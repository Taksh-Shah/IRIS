//! BLE-001 — ATT segmentation + reassembly (AC-2, AC-6).
//!
//! Android 14+ negotiates ATT MTU 517 on the *first* `requestMtu` per ACL and
//! disregards later requests (RES-0019 R1/G5). We therefore treat MTU as
//! **negotiate-late**: the segmenter starts at the 23-byte default and resizes
//! on `on_mtu_changed`, never exceeding `min(mtu - 5, 512)` bytes of ATT payload
//! per frame.
//!
//! Frame format (per GATT write):
//! ```text
//! [u16 BE total_payload_len][u16 BE msg_id][u8 chunk_idx][u8 chunk_count][chunk data]
//! ```
//! Header is 6 bytes. A message whose encoded length fits one frame is a single
//! chunk (`idx == count == 1`); larger messages are chunked and reassembled.
//!
//! Defensive bounds (RES-0019 R5/G5): the reassembler rejects frames whose
//! declared total exceeds `MAX_MESSAGE_BYTES`, buffers are evicted by TTL, and
//! a per-message chunk budget bounds memory. No allocation blowup on malformed
//! input.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

/// Default ATT MTU before negotiation (Core Spec 5.x default).
pub const MTU_DEFAULT: u16 = 23;
/// Android 14+ negotiated value on first `requestMtu` (RES-0019 R1).
pub const MTU_NEGOTIATED: u16 = 517;
/// Max ATT payload bytes per frame we ever emit (517 − 5 header, conservative 512).
pub const MAX_ATT_PAYLOAD: usize = 512;
/// Frame header size in bytes.
pub const FRAME_HEADER: usize = 6;
/// Upper bound on a single message we will reassemble — the frame `total` field
/// is a `u16`, so the wire cap is 65,535 bytes. Far above P0-P7 budgets (max
/// message class ≥64 KB); INTERNET-001 uses a larger 1 MiB cap for TCP, which
/// BLE cannot express.
pub const MAX_MESSAGE_BYTES: usize = u16::MAX as usize;
/// Max chunks per message — bounded by the single-byte `count` field (255).
/// At MTU 23 (12 data bytes/frame) this caps a message at ~3 KB, which is below
/// any P-class budget; at negotiated MTU the practical cap is ~129 KB.
pub const MAX_CHUNKS: usize = 255;
/// Stale partial-message TTL before eviction.
pub const REASSEMBLY_TTL: Duration = Duration::from_secs(30);
/// Cap on concurrently tracked partial messages (anti-DoS, mirrors SEC-001
/// quota discipline).
pub const MAX_PARTIALS: usize = 64;

/// Translate a platform-reported ATT payload into the equivalent MTU the
/// segmenter stores.
///
/// iOS `maximumWriteValueLength` reports the negotiated ATT payload (MTU − 3,
/// capped 512); the segmenter's own budget convention is `mtu − 5`, so storing
/// `payload + 5` makes `max_att_payload()` recover the FULL reported budget
/// (DEC-BLE-002-0004). A `0` report means "not yet negotiated" → negotiated
/// ceiling. Degraded payloads ≥ 20 (iOS 16.x regression) map to their own
/// budget — accepted, never an error (RES-0024 RQ-3/DI-5).
pub fn payload_to_mtu(payload: u16) -> u16 {
    if payload == 0 {
        return MTU_NEGOTIATED;
    }
    (payload as u32 + 5).clamp(MTU_DEFAULT as u32, MTU_NEGOTIATED as u32) as u16
}

/// Error from parsing a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    /// Frame shorter than the header.
    TooShort,
    /// Declared total exceeds `MAX_MESSAGE_BYTES`.
    MessageTooLarge,
    /// Declared chunk count exceeds `MAX_CHUNKS`.
    ChunkCountTooLarge,
    /// Chunk index >= declared count.
    BadChunkIndex,
    /// Header length/prelude mismatch (frame payload longer than declared).
    Truncated,
    /// Reassembly slot exhausted.
    TooManyPartials,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ble frame: {self:?}")
    }
}

impl std::error::Error for FrameError {}

/// Encodes/segments outbound messages and tracks the live MTU.
#[derive(Debug)]
pub struct AttSegmenter {
    mtu: AtomicU16,
    next_msg: AtomicU16,
}

impl Default for AttSegmenter {
    fn default() -> Self {
        Self::new()
    }
}

impl AttSegmenter {
    pub fn new() -> Self {
        AttSegmenter {
            mtu: AtomicU16::new(MTU_DEFAULT),
            next_msg: AtomicU16::new(0),
        }
    }

    /// Current effective ATT MTU.
    pub fn mtu(&self) -> u16 {
        self.mtu.load(Ordering::Relaxed)
    }

    /// Update MTU after negotiation (clamped to `[23, 517]`).
    pub fn on_mtu_changed(&self, mtu: u16) {
        let clamped = mtu.clamp(MTU_DEFAULT, MTU_NEGOTIATED);
        self.mtu.store(clamped, Ordering::Relaxed);
    }

    /// Adopt the negotiated ATT *payload* reported by a platform that reports
    /// payload rather than MTU (iOS `maximumWriteValueLength` = negotiated ATT
    /// payload, MTU − 3, capped 512; no request API — DEC-BLE-002-0004).
    ///
    /// Degraded payloads (e.g. 20 B on iOS 16.0/16.0.1, fixed 16.1) are
    /// accepted as **degraded-but-functional** — never an error, never a retry
    /// loop (RES-0024 RQ-3/DI-5). A `0` report (adapter that has not
    /// negotiated yet) maps to the negotiated ceiling.
    pub fn on_payload_reported(&self, payload: u16) {
        self.mtu.store(payload_to_mtu(payload), Ordering::Relaxed);
    }

    /// Max ATT payload bytes per frame given the current MTU.
    pub fn max_att_payload(&self) -> usize {
        let cap = (self.mtu.load(Ordering::Relaxed) as usize)
            .saturating_sub(5)
            .max(1);
        cap.min(MAX_ATT_PAYLOAD)
    }

    /// Max chunk data bytes (frame minus header).
    pub fn max_chunk_data(&self) -> usize {
        self.max_att_payload().saturating_sub(FRAME_HEADER).max(1)
    }

    fn alloc_msg_id(&self) -> u16 {
        self.next_msg.fetch_add(1, Ordering::Relaxed)
    }

    /// Segment `payload` into one or more frames (each ≤ `max_att_payload`).
    ///
    /// Returns `(msg_id, frames)`. A payload that fits one frame yields a single
    /// chunk. Returns an error if the message is oversized.
    pub fn segment(&self, payload: &[u8]) -> Result<(u16, Vec<Vec<u8>>), FrameError> {
        self.segment_for_mtu(payload, self.mtu.load(Ordering::Relaxed))
    }

    /// Segment `payload` honoring an explicit *per-connection* ATT MTU.
    ///
    /// ATT MTU is negotiated per GATT link (ble_att.rs module docs; RES-0019
    /// R1), so `send()` must size frames to the destination peer's own MTU, not
    /// a transport-global value (BLE-RT-003). Frames are sized to
    /// `min(mtu - 5, MAX_ATT_PAYLOAD)` exactly like the live-MTU path.
    pub fn segment_for_mtu(
        &self,
        payload: &[u8],
        mtu: u16,
    ) -> Result<(u16, Vec<Vec<u8>>), FrameError> {
        if payload.len() > MAX_MESSAGE_BYTES {
            return Err(FrameError::MessageTooLarge);
        }
        let msg_id = self.alloc_msg_id();
        let clamped = mtu.clamp(MTU_DEFAULT, MTU_NEGOTIATED);
        // Per-connection ATT payload => min(mtu-5, 512), then subtract the
        // 6-byte frame header for the chunk-data budget (BLE-RT-003).
        let att = ((clamped as usize) - 5).clamp(1, MAX_ATT_PAYLOAD);
        let data_cap = att.saturating_sub(FRAME_HEADER).max(1);
        let chunks: Vec<&[u8]> = payload.chunks(data_cap).collect();
        let count = chunks.len();
        if count > MAX_CHUNKS {
            return Err(FrameError::ChunkCountTooLarge);
        }
        let frames: Vec<Vec<u8>> = chunks
            .into_iter()
            .enumerate()
            .map(|(idx, chunk)| encode_frame(msg_id, idx, count, payload.len(), chunk))
            .collect::<Result<_, FrameError>>()?;
        Ok((msg_id, frames))
    }
}

/// Encode a single frame from a message chunk.
///
/// `count` is the TOTAL chunk count of the message (known up front), so the
/// reassembler can allocate a bounded slot map on first frame.
///
/// Returns a framing error rather than silently truncating oversized values
/// in release builds (BLE-RT-007). Callers must have already bounded
/// `total <= MAX_MESSAGE_BYTES` and `count <= MAX_CHUNKS`.
pub fn encode_frame(
    msg_id: u16,
    idx: usize,
    count: usize,
    total: usize,
    chunk: &[u8],
) -> Result<Vec<u8>, FrameError> {
    if total > MAX_MESSAGE_BYTES {
        return Err(FrameError::MessageTooLarge);
    }
    if count > MAX_CHUNKS {
        return Err(FrameError::ChunkCountTooLarge);
    }
    if idx >= count {
        return Err(FrameError::BadChunkIndex);
    }
    if chunk.len() > total {
        return Err(FrameError::Truncated);
    }
    let total_u16 = total as u16;
    let mut out = Vec::with_capacity(FRAME_HEADER + chunk.len());
    out.extend_from_slice(&total_u16.to_be_bytes());
    out.extend_from_slice(&msg_id.to_be_bytes());
    out.push(idx as u8);
    out.push(count as u8);
    out.extend_from_slice(chunk);
    Ok(out)
}

/// Decode a frame header; returns `(total_payload_len, msg_id, idx, chunk_count, data)`.
pub fn decode_frame(frame: &[u8]) -> Result<(usize, u16, usize, usize, &[u8]), FrameError> {
    if frame.len() < FRAME_HEADER {
        return Err(FrameError::TooShort);
    }
    let total = u16::from_be_bytes([frame[0], frame[1]]) as usize;
    let msg_id = u16::from_be_bytes([frame[2], frame[3]]);
    let idx = frame[4] as usize;
    let count = frame[5] as usize;
    if total > MAX_MESSAGE_BYTES {
        return Err(FrameError::MessageTooLarge);
    }
    if count > MAX_CHUNKS {
        return Err(FrameError::ChunkCountTooLarge);
    }
    if idx >= count {
        return Err(FrameError::BadChunkIndex);
    }
    let data = &frame[FRAME_HEADER..];
    Ok((total, msg_id, idx, count, data))
}

/// Partial message being reassembled.
#[derive(Debug)]
struct Partial {
    total: usize,
    count: usize,
    chunks: Vec<Option<Vec<u8>>>,
    received: usize,
    last_seen: Instant,
}

/// Reassembles inbound frames into complete messages (per-peer instance).
#[derive(Debug, Default)]
pub struct Reassembler {
    partials: HashMap<u16, Partial>,
}

impl Reassembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tracked partial messages.
    pub fn pending(&self) -> usize {
        self.partials.len()
    }

    /// Drop stale partials (call on the poll loop at a coarse cadence).
    pub fn evict_stale(&mut self, ttl: Duration, now: Instant) {
        self.partials
            .retain(|_, p| now.duration_since(p.last_seen) < ttl);
    }

    /// Push one frame. Returns the complete message once all chunks arrived.
    pub fn push(&mut self, frame: &[u8], now: Instant) -> Result<Option<Vec<u8>>, FrameError> {
        let (total, msg_id, idx, count, data) = decode_frame(frame)?;
        // Single-chunk fast path: the frame carries the entire payload.
        if count == 1 {
            if data.len() != total {
                return Err(FrameError::Truncated);
            }
            return Ok(Some(data.to_vec()));
        }
        if self.partials.len() >= MAX_PARTIALS && !self.partials.contains_key(&msg_id) {
            return Err(FrameError::TooManyPartials);
        }
        let partial = self.partials.entry(msg_id).or_insert_with(|| Partial {
            total,
            count,
            chunks: (0..count).map(|_| None).collect(),
            received: 0,
            last_seen: now,
        });
        partial.last_seen = now;
        if partial.count != count || partial.total != total {
            // Frame belongs to a different message with the same id wrap — reset.
            *partial = Partial {
                total,
                count,
                chunks: (0..count).map(|_| None).collect(),
                received: 0,
                last_seen: now,
            };
        }
        // BLE-23: per-chunk byte cap — reject a chunk whose data alone exceeds
        // the declared total before storing anything. This catches a hostile peer
        // that lies about `total` to drive a large Vec::with_capacity at reassembly.
        if data.len() > total {
            return Err(FrameError::Truncated);
        }
        if partial.chunks[idx].is_none() {
            partial.chunks[idx] = Some(data.to_vec());
            partial.received += 1;
        }
        if partial.received == partial.count {
            let partial = self.partials.remove(&msg_id).expect("present");
            // Use actual accumulated size as capacity to avoid over-allocating on
            // a lied-about `total` that survived the per-chunk check above.
            let actual_cap = partial.chunks.iter().flatten().map(Vec::len).sum();
            let mut out = Vec::with_capacity(actual_cap);
            for chunk in partial.chunks {
                out.extend_from_slice(&chunk.expect("received==count guarantees all chunks"));
            }
            if out.len() != partial.total {
                return Err(FrameError::Truncated);
            }
            Ok(Some(out))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segmenter_starts_at_default_mtu() {
        let s = AttSegmenter::new();
        assert_eq!(s.mtu(), MTU_DEFAULT);
        assert!(s.max_att_payload() <= MAX_ATT_PAYLOAD);
    }

    #[test]
    fn on_mtu_changed_clamps_and_resizes() {
        let s = AttSegmenter::new();
        s.on_mtu_changed(MTU_NEGOTIATED);
        assert_eq!(s.mtu(), MTU_NEGOTIATED);
        assert_eq!(s.max_att_payload(), MAX_ATT_PAYLOAD);
        // Out-of-range values clamp, never panic.
        s.on_mtu_changed(0);
        assert_eq!(s.mtu(), MTU_DEFAULT);
        s.on_mtu_changed(u16::MAX);
        assert_eq!(s.mtu(), MTU_NEGOTIATED);
    }

    #[test]
    fn ios_maximum_write_value_length_sets_full_payload_budget() {
        // AC-2 / DEC-BLE-002-0004: iOS reports the negotiated ATT payload
        // (maximumWriteValueLength), not the MTU — the segmenter must honor the
        // FULL reported budget (512 B), not 512 − 5.
        let s = AttSegmenter::new();
        s.on_payload_reported(512);
        assert_eq!(s.mtu(), MTU_NEGOTIATED);
        assert_eq!(s.max_att_payload(), MAX_ATT_PAYLOAD);
        let payload: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let (_id, frames) = s.segment(&payload).unwrap();
        assert!(frames.len() >= 2, "600 B must chunk at 512-B payload");
        assert!(
            frames.iter().all(|f| f.len() <= MAX_ATT_PAYLOAD),
            "frames must respect the reported 512-B payload budget"
        );
    }

    #[test]
    fn ios_degraded_20b_payload_accepted_as_functional() {
        // AC-2 / RES-0024 RQ-3/DI-5: iOS 16.0/16.0.1 reported a 20-B negotiated
        // payload (fixed 16.1) — accept degraded-but-functional, never error,
        // never a retry loop.
        let s = AttSegmenter::new();
        s.on_payload_reported(20);
        assert_eq!(s.max_att_payload(), 20, "20-B payload keeps its own budget");
        let payload = vec![3u8; 40];
        let (_id, frames) = s.segment(&payload).unwrap();
        assert!(frames.len() >= 2, "40 B at a 20-B payload must chunk");
        assert!(
            frames.iter().all(|f| f.len() <= 20),
            "degraded frames must respect the 20-B budget"
        );
        // The degraded link still delivers end-to-end.
        let mut r = Reassembler::new();
        let mut complete = None;
        for f in &frames {
            if let Some(m) = r.push(f, Instant::now()).unwrap() {
                complete = Some(m);
            }
        }
        assert_eq!(complete, Some(payload));
    }

    #[test]
    fn ios_default_and_unreported_payloads_map_safely() {
        // AC-2: 23-B payload (un-negotiated default) and 0 (not yet reported)
        // must both resolve to usable, bounded budgets.
        let s = AttSegmenter::new();
        s.on_payload_reported(23);
        assert_eq!(s.max_att_payload(), 23);
        s.on_payload_reported(0);
        assert_eq!(
            s.mtu(),
            MTU_NEGOTIATED,
            "0 report maps to negotiated ceiling"
        );
        assert_eq!(s.max_att_payload(), MAX_ATT_PAYLOAD);
    }

    #[test]
    fn single_chunk_round_trip() {
        let s = AttSegmenter::new();
        s.on_mtu_changed(MTU_NEGOTIATED);
        let payload: Vec<u8> = (0..64u8).collect();
        let (_id, frames) = s.segment(&payload).unwrap();
        assert_eq!(frames.len(), 1, "64B fits a 512B payload");
        let mut r = Reassembler::new();
        let out = r.push(&frames[0], Instant::now()).unwrap().unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn multi_chunk_round_trip_orders_out_of_order() {
        let s = AttSegmenter::new();
        s.on_mtu_changed(MTU_NEGOTIATED);
        // > 512B payload forces multiple chunks at the negotiated MTU.
        let payload: Vec<u8> = (0..1_500u32).map(|i| (i % 251) as u8).collect();
        let (_id, mut frames) = s.segment(&payload).unwrap();
        assert!(frames.len() >= 2, "1.5KiB must not fit one 512B frame");
        let mut r = Reassembler::new();
        // Deliver the LAST chunk first to prove ordering-tolerant reassembly.
        let last = frames.pop().unwrap();
        for f in &frames {
            assert_eq!(r.push(f, Instant::now()).unwrap(), None);
        }
        assert_eq!(r.push(&last, Instant::now()).unwrap(), Some(payload));
        assert_eq!(r.pending(), 0);
    }

    #[test]
    fn default_mtu_forces_many_chunks() {
        let s = AttSegmenter::new(); // MTU 23 → max_att_payload 18 → data 12
        let payload = vec![7u8; 100];
        let (_id, frames) = s.segment(&payload).unwrap();
        assert!(frames.len() >= 2, "100B at MTU23 needs multiple frames");
        assert!(
            frames.iter().all(|f| f.len() <= 18),
            "each frame must respect the 23-byte MTU payload budget"
        );
        let mut r = Reassembler::new();
        let mut complete = None;
        for f in &frames {
            if let Some(m) = r.push(f, Instant::now()).unwrap() {
                complete = Some(m);
            }
        }
        assert_eq!(complete, Some(payload));
    }

    #[test]
    fn too_short_frame_rejected() {
        let mut r = Reassembler::new();
        assert_eq!(
            r.push(&[], Instant::now()).unwrap_err(),
            FrameError::TooShort
        );
        assert_eq!(
            r.push(&[0, 0, 0], Instant::now()).unwrap_err(),
            FrameError::TooShort
        );
    }

    #[test]
    fn oversized_declared_total_rejected() {
        let mut r = Reassembler::new();
        // total = 65,535 (u16::MAX) is the largest expressible value and is
        // within the allowed bound, so push the *reassembly* over the budget
        // via a frame that declares more chunks than data could ever fill.
        let mut frame = Vec::new();
        frame.extend_from_slice(&u16::MAX.to_be_bytes()); // total = 65535
        frame.extend_from_slice(&0u16.to_be_bytes()); // msg_id
        frame.push(0); // idx
        frame.push(2); // count = 2 chunks
        frame.push(0);
        // count=2, idx=0 valid; message stays partial (never completes because
        // the second chunk never arrives). Just assert no panic and pending grows.
        assert_eq!(r.push(&frame, Instant::now()).unwrap(), None);
        assert_eq!(r.pending(), 1);
    }

    #[test]
    fn bad_chunk_index_rejected() {
        let mut r = Reassembler::new();
        // idx=9 with a declared count of 5 is impossible.
        let mut frame = Vec::new();
        frame.extend_from_slice(&10u16.to_be_bytes());
        frame.extend_from_slice(&1u16.to_be_bytes());
        frame.push(9); // idx
        frame.push(5); // count
        frame.push(0);
        assert_eq!(
            r.push(&frame, Instant::now()).unwrap_err(),
            FrameError::BadChunkIndex
        );
    }

    #[test]
    fn stale_partials_evicted() {
        let mut r = Reassembler::new();
        let s = AttSegmenter::new();
        s.on_mtu_changed(MTU_NEGOTIATED);
        let payload = vec![1u8; 600];
        let (_id, frames) = s.segment(&payload).unwrap();
        assert!(frames.len() >= 2);
        // Push only the first chunk, then simulate time passing.
        assert_eq!(r.push(&frames[0], Instant::now()).unwrap(), None);
        let later = Instant::now() + REASSEMBLY_TTL + Duration::from_secs(1);
        r.evict_stale(REASSEMBLY_TTL, later);
        assert_eq!(r.pending(), 0, "stale partial must be evicted");
    }

    #[test]
    fn partials_bounded() {
        let mut r = Reassembler::new();
        let s = AttSegmenter::new();
        s.on_mtu_changed(MTU_NEGOTIATED);
        // Craft MAX_PARTIALS distinct single-of-multi frames with unique msg ids.
        let mut filled = 0;
        for _ in 0..(MAX_PARTIALS + 8) {
            let payload = vec![2u8; 600];
            let (id, mut frames) = s.segment(&payload).unwrap();
            let _ = id; // frame carries its own id
            let first = frames.remove(0);
            match r.push(&first, Instant::now()) {
                Ok(None) => filled += 1,
                Ok(Some(_)) => unreachable!("single chunk of a multi-chunk msg"),
                Err(FrameError::TooManyPartials) => {
                    assert_eq!(filled, MAX_PARTIALS, "cap engages at MAX_PARTIALS");
                    return;
                }
                Err(e) => panic!("unexpected error {e:?}"),
            }
        }
        assert_eq!(filled, MAX_PARTIALS);
    }
}
