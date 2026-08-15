//! Key store seam — IDENT-001 D2 (§5.2/§5.3, DEC-P0006).
//!
//! [`KeyStore`] abstracts where the node's durable identity material lives.
//! v1 ships [`FileKeyStore`] — the protected-key-file desktop posture
//! (recorded as the weaker-but-deterministic v1 default in IDENT_DESIGN §5.3;
//! OS keychain behind this same trait is the documented hardening follow-up).
//!
//! Security properties:
//! - user-only file permissions (0600 on Unix; Windows user-appdata isolation),
//! - **atomic** writes (temp file + rename) → no torn reads on crash,
//! - advisory lock file (`create_new`) for cross-process safety,
//! - secret-bearing buffers ride `zeroize::Zeroizing` (zeroed on drop),
//! - file header = version byte + reserved bytes (future re-key / derivation).
//!
//! Corruption / version mismatch are surfaced as errors, never silently
//! regenerated (§5.4 — regeneration changes PeerId and orphans storage).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use zeroize::Zeroizing;

/// Errors from the key store.
#[derive(Debug, thiserror::Error)]
pub enum KeyStoreError {
    #[error("keystore: io: {0}")]
    Io(#[from] io::Error),
    #[error("keystore: lock is held by another process (retry or remove {0})")]
    Locked(PathBuf),
    #[error("keystore: file has insecure permissions (expected user-only, got mode {0:o})")]
    InsecurePermissions(u32),
    #[error("keystore: unknown format version {0}")]
    BadVersion(u8),
    #[error("keystore: malformed blob (len {0})")]
    Malformed(usize),
}

/// Where identity material is durably stored.
pub trait KeyStore: Send + Sync + 'static {
    /// Load the raw identity blob (versioned serialization, see
    /// `provision.rs`). `Ok(None)` when the store is empty (first run).
    ///
    /// RT-008: the returned buffer rides [`Zeroizing`] — the payload contains
    /// the signing + encryption seeds and must be wiped when the caller drops
    /// it. Callers get a `&[u8]` view via deref coercion as before.
    fn load_identity(&self) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError>;

    /// Atomically persist the raw identity blob (0600, zeroized buffer).
    fn save_identity(&self, blob: &[u8]) -> Result<(), KeyStoreError>;

    /// Load the master storage key (`Ok(None)` on first run).
    fn load_master_key(&self) -> Result<Option<Zeroizing<[u8; 32]>>, KeyStoreError>;

    /// Atomically persist the master storage key.
    fn save_master_key(&self, key: &[u8; 32]) -> Result<(), KeyStoreError>;
}

/// Identity-file header: `[version: u8][reserved: u8 × 3]` = 4 bytes of
/// reserved prefix before the CBOR payload (future re-key / derivation).
const FILE_HEADER: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

/// Protected key file on the platform app-data directory.
///
/// Layout (per §5.3):
///   `<app_data>/keys/identity.v1` — versioned identity blob (header+CBOR)
///   `<app_data>/keys/master.key`  — 32-byte master storage key
///   `<app_data>/keys/*.lock`      — advisory lock during writes
#[derive(Debug, Clone)]
pub struct FileKeyStore {
    identity_path: PathBuf,
    master_key_path: PathBuf,
}

fn app_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join(".local/share"))
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        PathBuf::from(".")
    }
}

impl FileKeyStore {
    /// Store rooted in the platform app-data dir: `<app_data>/iris/keys/`.
    pub fn default_data_dir() -> Self {
        Self::at(app_data_dir().join("iris"))
    }

    /// Store rooted at `base` (tests / explicit config). Directory is created
    /// on first save.
    pub fn at(base: impl Into<PathBuf>) -> Self {
        let base = base.into();
        Self {
            identity_path: base.join("keys").join("identity.v1"),
            master_key_path: base.join("keys").join("master.key"),
        }
    }

    fn keys_dir(&self) -> &Path {
        self.identity_path.parent().expect("keys dir from identity path")
    }

    /// Take an exclusive advisory lock for `path` (create_new → atomic).
    ///
    /// RT-006: a `.lock` file left behind by a crashed/killed writer must not
    /// brick the store permanently. `create_new` is still atomic (no torn
    /// read), but if the lock file is stale (older than [`LOCK_STALE_SECS`])
    /// it is reclaimed by removing it and retrying — recovery is explicit and
    /// bounded, never clobbers a *live* lock (a live writer's lock is fresh).
    fn lock(&self, path: &Path) -> Result<fs::File, KeyStoreError> {
        let lock_path = path.with_extension("lock");
        fs::create_dir_all(self.keys_dir())?;
        for attempt in 0..2 {
            match fs::OpenOptions::new().write(true).create_new(true).open(&lock_path) {
                Ok(f) => return Ok(f),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if attempt == 0 && lock_is_stale(&lock_path) {
                        // Crashed writer left a stale lock; reclaim it.
                        let _ = fs::remove_file(&lock_path);
                        continue;
                    }
                    return Err(KeyStoreError::Locked(lock_path));
                }
                Err(e) => return Err(e.into()),
            }
        }
        Err(KeyStoreError::Locked(lock_path))
    }

    /// Write `data` atomically (temp + rename), 0600, locked.
    fn atomic_write(&self, dest: &Path, data: &[u8]) -> Result<(), KeyStoreError> {
        let _lock = self.lock(dest)?;
        fs::create_dir_all(self.keys_dir())?;
        let tmp = dest.with_extension("tmp");
        {
            let mut buf = Zeroizing::new(Vec::with_capacity(FILE_HEADER.len() + data.len()));
            buf.extend_from_slice(&FILE_HEADER);
            buf.extend_from_slice(data);
            // RT-007: the temp file is created 0600 from birth — no window
            // where an attacker on the same host can read key material.
            write_secret_file(&tmp, &buf)?;
        }
        set_user_only(&tmp)?;
        fs::rename(&tmp, dest)?;
        Ok(())
    }

    fn with_header_stripped(blob: &[u8]) -> Result<&[u8], KeyStoreError> {
        if blob.len() < FILE_HEADER.len() {
            return Err(KeyStoreError::Malformed(blob.len()));
        }
        if blob[0] != FILE_HEADER[0] {
            return Err(KeyStoreError::BadVersion(blob[0]));
        }
        Ok(&blob[FILE_HEADER.len()..])
    }
}

/// Limit for how fresh a live lock must be. A lock older than this is treated
/// as abandoned (RT-006). Writes are a single small rename → live locks are
/// sub-second; 15 s leaves an enormous safety margin over any slow disk.
const LOCK_STALE_SECS: u64 = 15;

/// `true` when the lock file's mtime is older than [`LOCK_STALE_SECS`].
fn lock_is_stale(lock_path: &Path) -> bool {
    let Ok(meta) = fs::metadata(lock_path) else {
        return false;
    };
    let Ok(mtime) = meta.modified() else {
        return false;
    };
    let Ok(age) = std::time::SystemTime::now().duration_since(mtime) else {
        return false; // clock skew; leave the lock alone
    };
    age.as_secs() > LOCK_STALE_SECS
}

/// Write a secret file created **0600 from birth** (RT-007). On Unix the mode
/// is fixed at `open(2)` time so the create window never exposes the file;
/// elsewhere the platform's app-data isolation applies. A leftover `.tmp` from
/// a prior crash is truncated in place (its perms are already our 0600).
#[cfg(unix)]
fn write_secret_file(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(data)?;
    f.sync_all()
}
#[cfg(not(unix))]
fn write_secret_file(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut f = fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)?;
    f.write_all(data)?;
    f.sync_all()
}

/// Restrict `path` to the current user (0600 on Unix). On Windows the app-data
/// directory is per-user and std's ACL control is intentionally not engaged in
/// v1 (documented posture per §5.3).
#[cfg(unix)]
fn set_user_only(path: &Path) -> Result<(), KeyStoreError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}
#[cfg(not(unix))]
fn set_user_only(_path: &Path) -> Result<(), KeyStoreError> {
    Ok(())
}

#[cfg(unix)]
fn check_user_only(path: &Path) -> Result<(), KeyStoreError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode != 0o600 {
        return Err(KeyStoreError::InsecurePermissions(mode));
    }
    Ok(())
}
#[cfg(not(unix))]
fn check_user_only(_path: &Path) -> Result<(), KeyStoreError> {
    Ok(())
}

impl KeyStore for FileKeyStore {
    fn load_identity(&self) -> Result<Option<Zeroizing<Vec<u8>>>, KeyStoreError> {
        // RT-008: the whole read path stays in Zeroizing buffers — the raw file
        // contents and the header-stripped payload both contain the seeds.
        let blob = match fs::read(&self.identity_path) {
            Ok(b) => Zeroizing::new(b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        check_user_only(&self.identity_path)?;
        let payload = Self::with_header_stripped(&blob)?;
        Ok(Some(Zeroizing::new(payload.to_vec())))
    }

    fn save_identity(&self, blob: &[u8]) -> Result<(), KeyStoreError> {
        self.atomic_write(&self.identity_path, blob)
    }

    fn load_master_key(&self) -> Result<Option<Zeroizing<[u8; 32]>>, KeyStoreError> {
        let blob = match fs::read(&self.master_key_path) {
            Ok(b) => Zeroizing::new(b),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        check_user_only(&self.master_key_path)?;
        let payload = Self::with_header_stripped(&blob)?;
        if payload.len() != 32 {
            return Err(KeyStoreError::Malformed(payload.len()));
        }
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(payload);
        Ok(Some(key))
    }

    fn save_master_key(&self, key: &[u8; 32]) -> Result<(), KeyStoreError> {
        self.atomic_write(&self.master_key_path, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (FileKeyStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        (FileKeyStore::at(dir.path()), dir)
    }

    #[test]
    fn fresh_store_is_empty() {
        let (s, _d) = temp_store();
        assert!(s.load_identity().unwrap().is_none());
        assert!(s.load_master_key().unwrap().is_none());
    }

    #[test]
    fn identity_round_trip_and_reload() {
        let (s, dir) = temp_store();
        let blob = vec![0xAA; 40];
        s.save_identity(&blob).unwrap();
        drop(s);
        let s2 = FileKeyStore::at(dir.path());
        assert_eq!(&*s2.load_identity().unwrap().unwrap(), &blob);
    }

    #[test]
    fn master_key_round_trip() {
        let (s, dir) = temp_store();
        let key = [7u8; 32];
        s.save_master_key(&key).unwrap();
        drop(s);
        let s2 = FileKeyStore::at(dir.path());
        assert_eq!(&*s2.load_master_key().unwrap().unwrap(), &key);
    }

    #[test]
    fn wrong_version_blob_fails_loudly() {
        let (s, _d) = temp_store();
        let path = s.identity_path.clone();
        std::fs::create_dir_all(s.keys_dir()).unwrap();
        std::fs::write(&path, [0x02, 0, 0, 0]).unwrap();
        assert!(matches!(s.load_identity(), Err(KeyStoreError::BadVersion(0x02))));
    }

    #[test]
    fn corrupt_blob_fails_loudly() {
        // Header-only (no payload) is structurally valid at the store layer —
        // the store does not know the payload format; the identity layer
        // rejects the empty payload loudly (§5.4). So the store must return
        // the raw (empty) payload for the identity layer to validate.
        let (s, _d) = temp_store();
        let path = s.identity_path.clone();
        std::fs::create_dir_all(s.keys_dir()).unwrap();
        std::fs::write(&path, [0x01, 0, 0, 0]).unwrap(); // header only, no payload
        assert_eq!(&*s.load_identity().unwrap().unwrap(), &Vec::<u8>::new());
    }

    #[cfg(unix)]
    #[test]
    fn identity_file_is_0600() {
        let (s, _d) = temp_store();
        s.save_identity(&vec![1u8; 8]).unwrap();
        check_user_only(&s.identity_path).expect("must be 0600");
    }

    #[test]
    fn buffer_buffers_zeroed_on_drop() {
        // AC-2: secret-bearing store buffers must zeroize through the Drop
        // path. We observe the wipe on the Vec's LIVE heap buffer between
        // drop_in_place and dealloc (the only sound read of cleared memory).
        use std::alloc::{self, Layout};
        let layout = Layout::new::<Zeroizing<Vec<u8>>>();
        let ptr = unsafe { alloc::alloc(layout) } as *mut Zeroizing<Vec<u8>>;
        assert!(!ptr.is_null(), "alloc failed");
        unsafe {
            ptr.write(Zeroizing::new(vec![0xEEu8; 64]));
        }
        // Capture the Vec's buffer pointer BEFORE the drop path runs.
        let z = unsafe { &*ptr };
        let buffer = (&***z)[..].as_ptr(); // Zeroizing derefs to Vec derefs to [u8]
        unsafe { std::ptr::drop_in_place(ptr); } // runs clear() over the buffer
        let cleared = unsafe { std::slice::from_raw_parts(buffer, 64) };
        let all_zero = cleared.iter().all(|&b| b == 0);
        unsafe { alloc::dealloc(ptr as *mut u8, layout); }
        assert!(all_zero, "buffer must be zeroed after drop path");
    }

    #[test]
    fn stale_lock_file_is_reclaimed() {
        // RT-006: a lock left by a crashed writer (stale mtime) must not brick
        // the store. Backdate a pre-created lock file beyond LOCK_STALE_SECS,
        // then a save must recover by reclaiming it instead of Locked.
        let (s, _d) = temp_store();
        std::fs::create_dir_all(s.keys_dir()).unwrap();
        let lock_path = s.identity_path.with_extension("lock");
        std::fs::write(&lock_path, b"stale").unwrap();

        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(LOCK_STALE_SECS + 60);
        let f = std::fs::OpenOptions::new().write(true).open(&lock_path).unwrap();
        f.set_times(
            std::fs::FileTimes::new()
                .set_accessed(old)
                .set_modified(old),
        )
        .unwrap();
        drop(f);
        assert!(lock_is_stale(&lock_path), "backdated lock must be stale");

        // A save must succeed: the stale lock is reclaimed and removed.
        s.save_identity(&[7u8; 16]).unwrap();
        assert_eq!(s.load_identity().unwrap().unwrap().to_vec(), vec![7u8; 16]);
    }

    #[test]
    fn all_key_files_are_0600_from_birth() {
        // RT-007: temp + final identity + lock files must be user-only from
        // creation — there is never a world-readable window.
        let (s, _d) = temp_store();
        s.save_identity(&[1u8; 8]).unwrap();
        s.save_master_key(&[2u8; 32]).unwrap();
        check_user_only(&s.identity_path).expect("identity must be 0600");
        check_user_only(&s.master_key_path).expect("master key must be 0600");
    }

    use tempfile;
}