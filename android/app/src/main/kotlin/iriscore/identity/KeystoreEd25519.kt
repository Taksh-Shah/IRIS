package iriscore.identity

import android.content.Context
import android.os.Build
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import androidx.annotation.RequiresApi
import iriscore.util.PeerIdCodec
import java.io.ByteArrayInputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.security.KeyFactory
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.PublicKey
import java.security.Security
import java.security.Signature
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.X509EncodedKeySpec
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.bouncycastle.jce.provider.BouncyCastleProvider

/**
 * Registers the full BouncyCastle provider under the name "BC", used
 * explicitly (never relied on via provider search order) by every Ed25519
 * JCA call in [KeystoreEd25519.SoftwareBackend]. Android bundles its own
 * internal, TLS-only, non-public "BC" provider that does not implement
 * general-purpose asymmetric crypto for apps and would otherwise register
 * under the same name - remove it first so `Security.getProvider("BC")`
 * unambiguously resolves to the real one this app depends on. Idempotent
 * and safe to call from multiple entry points (JVM unit tests + the real
 * app); `Security.addProvider` is a no-op if a provider of the same name
 * is already registered, so a second removal+add would leave a
 * different-identity object in place - guard with a name check.
 */
internal object Ed25519Provider {
    val NAME: String = BouncyCastleProvider.PROVIDER_NAME

    fun ensureRegistered() {
        if (Security.getProvider(NAME) is BouncyCastleProvider) return
        Security.removeProvider(NAME)
        Security.insertProviderAt(BouncyCastleProvider(), 1)
    }
}

/**
 * AC-8 — Keystore TEE Ed25519 identity provisioning (D-7, RED-0005-aligned).
 *
 * Identity = one persistent Ed25519 keypair; the 32-byte raw public key IS the
 * 64-hex PeerId; sign/verify is the self-auth advertisement path. Session and
 * onion keys stay in the Rust engine (ephemeral — no HW anchor).
 *
 * Backends:
 *  - [AndroidKeystoreBackend] (API 33 floor, KeyMint v2 TEE): hardware-backed
 *    Ed25519 via `AndroidKeyStore`. StrongBox deliberately NOT used — it
 *    excludes Ed25519 (RES-0022 Q4 / G-AND-2).
 *  - [SoftwareBackend]: JCA `Ed25519` (JDK 15+/Conscrypt). The private key is
 *    wrapped under an AndroidKeyStore AES-GCM key (alias `iris_sw_wrap`) and
 *    persisted with the encoded public key in `getNoBackupFilesDir()`
 *    (AND-RT-102) — the identity is device-scoped and survives process restarts
 *    instead of rotating every boot. The no-arg constructor keeps runtime-only
 *    generation for the host-JVM test leg.
 *
 * [KeystoreEd25519(context)] auto-selects. JVM unit tests construct with
 * [SoftwareBackend] directly (the host Gradle `test` leg has no Android
 * Keystore).
 */
class KeystoreEd25519 internal constructor(
    private val backend: KeyBackend,
) {
    constructor(context: Context) : this(AutoBackend(context))

    enum class BackendType { TEE_HARDWARE, SOFTWARE }

    interface KeyBackend {
        val backendType: BackendType

        /** Create (once) or load the persistent keypair. */
        fun getOrCreateKeyPair(): KeyPair

        fun sign(privateKey: PrivateKey, data: ByteArray): ByteArray

        fun verify(publicKey: PublicKey, data: ByteArray, signature: ByteArray): Boolean

        fun publicKeyRaw(publicKey: PublicKey): ByteArray
    }

    /** AndroidKeyStore Ed25519 (API 33+). */
    @RequiresApi(33)
    class AndroidKeystoreBackend(
        private val context: Context,
        private val alias: String = KEYSTORE_ALIAS,
    ) : KeyBackend {

        override val backendType: BackendType = BackendType.TEE_HARDWARE

        private val keyStore: KeyStore by lazy {
            KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        }

        override fun getOrCreateKeyPair(): KeyPair {
            if (!keyStore.containsAlias(alias)) {
                // Was KeyProperties.KEY_ALGORITHM_EC with no curve specified,
                // which AndroidKeyStore defaults to NIST P-256 - a completely
                // different key type than Ed25519. The class doc above always
                // said "AndroidKeyStore Ed25519 (API 33+)"; the actual call
                // never requested it. Every downstream consumer (sign()/
                // verify() below via Signature.getInstance("Ed25519"),
                // PeerIdCodec.rawPoint() expecting a 32-byte raw point or a
                // 44-byte Ed25519 SPKI wrapper) assumed genuine Ed25519 and
                // got a P-256 key instead - rawPoint() then failed loudly on
                // P-256's 91-byte SPKI encoding rather than silently
                // misinterpreting it, which is why this surfaced as a crash
                // rather than a subtly wrong identity.
                // KeyProperties has no KEY_ALGORITHM_ED25519 constant on this
                // SDK (checked against the real android-35 platform stub) -
                // try the raw algorithm name string; AndroidKeyStore-backed
                // Ed25519 support (where present) is typically exposed this
                // way ahead of a formal KeyProperties constant.
                val kpg = KeyPairGenerator.getInstance(
                    "Ed25519",
                    "AndroidKeyStore",
                )
                val spec = KeyGenParameterSpec.Builder(
                    alias,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
                ).build()
                kpg.initialize(spec)
                kpg.generateKeyPair()
            }
            val public = keyStore.getCertificate(alias)?.publicKey
                ?: error("keystore certificate missing for $alias")
            val private = keyStore.getKey(alias, null) as? PrivateKey
                ?: error("keystore private key missing for $alias")
            return KeyPair(public, private)
        }

        override fun sign(privateKey: PrivateKey, data: ByteArray): ByteArray =
            Signature.getInstance("Ed25519").run {
                initSign(privateKey)
                update(data)
                sign()
            }

        override fun verify(publicKey: PublicKey, data: ByteArray, signature: ByteArray): Boolean =
            try {
                Signature.getInstance("Ed25519").run {
                    initVerify(publicKey)
                    update(data)
                    verify(signature)
                }
            } catch (_: Exception) {
                false
            }

        override fun publicKeyRaw(publicKey: PublicKey): ByteArray = PeerIdCodec.rawPoint(publicKey)
    }

    /**
     * JCA fallback (host JVM tests + older/no-TEE devices).
     *
     * With a [Context] the keypair is persisted (AND-RT-102): the Ed25519
     * private key is wrapped under an AndroidKeyStore AES-GCM key and stored
     * with the encoded public key in `getNoBackupFilesDir()` so the identity
     * survives process restarts. The no-arg constructor keeps runtime-only
     * generation for the host JVM test leg.
     */
    class SoftwareBackend private constructor(
        private val persisted: PersistedIdentityStore?,
    ) : KeyBackend {
        constructor() : this(null)

        internal constructor(context: Context) : this(PersistedIdentityStore(context))

        init {
            // Confirmed on a real API-31 device: Android's default JCA
            // provider stack does not implement Ed25519 below API 33, so
            // every call below must name BouncyCastle explicitly rather
            // than trust provider search order to find it.
            Ed25519Provider.ensureRegistered()
        }

        override val backendType: BackendType = BackendType.SOFTWARE

        private var generated: KeyPair? = null

        override fun getOrCreateKeyPair(): KeyPair {
            generated?.let { return it }
            val restored = persisted?.load() ?: generateAndPersist()
            generated = restored
            return restored
        }

        private fun generateAndPersist(): KeyPair {
            val fresh = KeyPairGenerator.getInstance("Ed25519", Ed25519Provider.NAME).generateKeyPair()
            if (persisted != null) {
                persisted.store(fresh)
            }
            return fresh
        }

        override fun sign(privateKey: PrivateKey, data: ByteArray): ByteArray =
            Signature.getInstance("Ed25519", Ed25519Provider.NAME).run {
                initSign(privateKey)
                update(data)
                sign()
            }

        override fun verify(publicKey: PublicKey, data: ByteArray, signature: ByteArray): Boolean =
            try {
                Signature.getInstance("Ed25519", Ed25519Provider.NAME).run {
                    initVerify(publicKey)
                    update(data)
                    verify(signature)
                }
            } catch (_: Exception) {
                false
            }

        override fun publicKeyRaw(publicKey: PublicKey): ByteArray = PeerIdCodec.rawPoint(publicKey)
    }

    /** Runtime pick: API 33+ with a working AndroidKeyStore -> TEE, else software. */
    class AutoBackend(private val context: Context) : KeyBackend {
        private val delegate: KeyBackend by lazy {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU && androidKeystoreEd25519Available()) {
                AndroidKeystoreBackend(context)
            } else {
                SoftwareBackend(context)
            }
        }

        // The API-33 floor alone does NOT mean AndroidKeyStore can generate an
        // Ed25519 key. Confirmed on a real API-34 device (Pixel-class,
        // KeyMint v2): `KeyPairGenerator.getInstance("Ed25519",
        // "AndroidKeyStore")` throws `NoSuchAlgorithmException: no such
        // algorithm: Ed25519 for provider AndroidKeyStore` - and
        // `KeyProperties` doesn't even have an ED25519 constant on this SDK
        // to name the algorithm the "supported" way. Probe for real support
        // rather than assuming API level implies it; the previous
        // unconditional-on-API-level selection produced a hard crash at
        // first launch on every device that reached this path, because
        // AndroidKeystoreBackend has no working fallback of its own.
        //
        // HW-8: constructibility alone is NOT proof of real support either.
        // Confirmed on a real Samsung Galaxy S24 Ultra (One UI 8 / API 36,
        // KeyMint): `KeyPairGenerator.getInstance("Ed25519",
        // "AndroidKeyStore")` and `initialize()` both succeed without
        // throwing, so the old "does construction throw" probe reported
        // Ed25519 as available - but the TEE silently generates a NIST
        // P-256 key instead (Samsung KeyMint apparently accepts "Ed25519"
        // as a spec-builder algorithm *name* without actually honoring it).
        // That reached `PeerIdCodec.rawPoint()` as a hard crash at every
        // launch: "IllegalStateException: unexpected point encoding size
        // 91" (91 bytes is P-256's X.509 SPKI DER size, not Ed25519's 32/44).
        // The only way to catch this is to actually generate a key and
        // inspect what came back, so the probe now does exactly that under
        // a disposable alias, deleted immediately after either way.
        private fun androidKeystoreEd25519Available(): Boolean = try {
            val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            try {
                val kpg = KeyPairGenerator.getInstance("Ed25519", "AndroidKeyStore")
                val spec = KeyGenParameterSpec.Builder(
                    PROBE_ALIAS,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
                ).build()
                kpg.initialize(spec)
                val probeKey = kpg.generateKeyPair().public.encoded
                probeKey.size == 32 || probeKey.size == 44
            } finally {
                // Best-effort cleanup — a leaked probe alias is inert (never
                // read by anything) but should not linger in the keystore.
                try {
                    keyStore.deleteEntry(PROBE_ALIAS)
                } catch (_: Exception) {
                }
            }
        } catch (_: Exception) {
            false
        }

        companion object {
            private const val PROBE_ALIAS = "iris_ed25519_probe_v1"
        }

        override val backendType: BackendType get() = delegate.backendType
        override fun getOrCreateKeyPair(): KeyPair = delegate.getOrCreateKeyPair()
        override fun sign(privateKey: PrivateKey, data: ByteArray): ByteArray = delegate.sign(privateKey, data)
        override fun verify(publicKey: PublicKey, data: ByteArray, signature: ByteArray): Boolean =
            delegate.verify(publicKey, data, signature)
        override fun publicKeyRaw(publicKey: PublicKey): ByteArray = delegate.publicKeyRaw(publicKey)
    }

    /**
     * Device-scoped persistence for the software identity (AND-RT-102).
     *
     * The Ed25519 private key (PKCS#8) is wrapped under a non-exportable
     * AndroidKeyStore AES-256-GCM key (`iris_sw_wrap`, ENCRYPT/DECRYPT only)
     * and stored with the encoded public key in `iris_sw_identity.bin` under
     * `getNoBackupFilesDir()` (excluded from cloud backup by construction).
     * A corrupt/unwrappable store fails loudly rather than silently rotating
     * the identity.
     */
    private class PersistedIdentityStore(private val context: Context) {

        private val file: File = File(context.getNoBackupFilesDir(), IDENTITY_FILE)

        private val keyStore: KeyStore by lazy {
            KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        }

        private val wrappingKey: SecretKey by lazy {
            if (!keyStore.containsAlias(WRAP_ALIAS)) {
                val generator = KeyGenerator.getInstance(
                    KeyProperties.KEY_ALGORITHM_AES,
                    "AndroidKeyStore",
                )
                val spec = KeyGenParameterSpec.Builder(
                    WRAP_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setKeySize(256)
                    .build()
                generator.init(spec)
                generator.generateKey()
            }
            keyStore.getKey(WRAP_ALIAS, null) as SecretKey
        }

        /** `null` when no identity has been persisted yet. */
        fun load(): KeyPair? {
            if (!file.exists()) return null
            try {
                val input = DataInputStream(ByteArrayInputStream(file.readBytes()))
                val version = input.readByte()
                if (version != FILE_VERSION) return null
                val iv = ByteArray(input.readInt()).also { input.readFully(it) }
                val wrapped = ByteArray(input.readInt()).also { input.readFully(it) }
                val public = ByteArray(input.readInt()).also { input.readFully(it) }
                val privatePkcs8 = unwrap(iv, wrapped)
                return decodeKeyPair(privatePkcs8, public)
            } catch (e: Exception) {
                throw IllegalStateException("failed to restore persisted IRIS identity", e)
            }
        }

        fun store(keyPair: KeyPair) {
            val privatePkcs8 = keyPair.private.encoded
            val public = keyPair.public.encoded
            val (iv, wrapped) = wrap(privatePkcs8)
            DataOutputStream(file.outputStream()).use { out ->
                out.writeByte(FILE_VERSION.toInt())
                out.writeInt(iv.size)
                out.write(iv)
                out.writeInt(wrapped.size)
                out.write(wrapped)
                out.writeInt(public.size)
                out.write(public)
            }
        }

        private fun wrap(plain: ByteArray): Pair<ByteArray, ByteArray> {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.ENCRYPT_MODE, wrappingKey)
            return cipher.iv to cipher.doFinal(plain)
        }

        private fun unwrap(iv: ByteArray, wrapped: ByteArray): ByteArray {
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, wrappingKey, GCMParameterSpec(128, iv))
            return cipher.doFinal(wrapped)
        }

        private fun decodeKeyPair(privatePkcs8: ByteArray, publicX509: ByteArray): KeyPair {
            // Explicit BC, same reasoning as SoftwareBackend's init block:
            // by the time this runs the caller (SoftwareBackend.getOrCreateKeyPair
            // -> load()) has always already constructed a SoftwareBackend, so
            // Ed25519Provider is already registered - call it again anyway,
            // it is a cheap idempotent check, not worth a fragile ordering
            // assumption between two classes.
            Ed25519Provider.ensureRegistered()
            val factory = KeyFactory.getInstance("Ed25519", Ed25519Provider.NAME)
            return KeyPair(
                factory.generatePublic(X509EncodedKeySpec(publicX509)),
                factory.generatePrivate(PKCS8EncodedKeySpec(privatePkcs8)),
            )
        }

        companion object {
            const val IDENTITY_FILE = "iris_sw_identity.bin"
            const val WRAP_ALIAS = "iris_sw_wrap"
            const val FILE_VERSION: Byte = 1
        }
    }

    private val keyPair: KeyPair by lazy { backend.getOrCreateKeyPair() }

    val backendType: BackendType get() = backend.backendType

    val publicKey: PublicKey get() = keyPair.public

    /** 32-byte raw Ed25519 public key == node identity (PeerId source). */
    fun publicKeyRaw(): ByteArray = backend.publicKeyRaw(publicKey)

    /** 64-hex node id. */
    fun nodeIdHex(): String = PeerIdCodec.toHex(publicKeyRaw())

    /** 16-hex compact id for UI. */
    fun shortIdHex(): String = PeerIdCodec.shortId(nodeIdHex())

    fun sign(data: ByteArray): ByteArray = backend.sign(keyPair.private, data)

    fun verify(data: ByteArray, signature: ByteArray): Boolean = backend.verify(publicKey, data, signature)

    companion object {
        const val KEYSTORE_ALIAS = "iris_identity_ed25519_v1"
    }
}