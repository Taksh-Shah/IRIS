package iriscore.identity

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import iriscore.util.PeerIdCodec
import java.io.ByteArrayInputStream
import java.io.ByteArrayOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.security.KeyFactory
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PublicKey
import java.security.spec.PKCS8EncodedKeySpec
import java.security.spec.X509EncodedKeySpec
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * AC-8 — X25519 static advertisement (RED-0005 two-keypair binding, D-7).
 *
 * Mirrors the IDENT-001 "Ed25519-signed-X25519" binding: the static ad is the
 * Ed25519 public key + an X25519 key-agreement public key, with the X25519 key
 * signed by the Ed25519 identity. Peers that have verified the identity
 * (TOFU/QR, RED-0005) can therefore trust the X25519 key for message-key
 * agreement without an additional trust path.
 *
 * X25519 key material is supplied through [X25519KeySupplier] (default
 * JCA `X25519`, JDK 11+/Conscrypt). HV-21: with a [Context] the keypair is
 * **persisted** — wrapped under an AndroidKeyStore AES-GCM key and stored in
 * `getNoBackupFilesDir()`, exactly like [KeystoreEd25519]'s software identity —
 * so a peer's registered X25519 key stays valid across app restarts (before
 * this, the key was `by lazy` random per process, so every cold start silently
 * invalidated every `/addkey`/friend entry and addressed messages stopped
 * decrypting until re-keyed by hand). The no-arg / no-context path keeps
 * runtime-only generation for the host-JVM `test` leg.
 */
class X25519StaticAd private constructor(
    private val ed25519: KeystoreEd25519,
    private val supplier: X25519KeySupplier,
    private val persisted: PersistedX25519Store?,
) {

    constructor(
        ed25519: KeystoreEd25519,
        supplier: X25519KeySupplier = JcaX25519KeySupplier(),
    ) : this(ed25519, supplier, null)

    constructor(
        ed25519: KeystoreEd25519,
        context: Context,
        supplier: X25519KeySupplier = JcaX25519KeySupplier(),
    ) : this(ed25519, supplier, PersistedX25519Store(context))

    data class StaticAdvertisement(
        val version: Int = VERSION,
        val ed25519PubRaw: ByteArray,
        val x25519PubRaw: ByteArray,
        val signature: ByteArray,
    )

    fun interface X25519KeySupplier {
        fun generateKeyPair(): KeyPair
    }

    class JcaX25519KeySupplier : X25519KeySupplier {
        override fun generateKeyPair(): KeyPair =
            KeyPairGenerator.getInstance("X25519").generateKeyPair()
    }

    /**
     * AND-RT-107: one stable X25519 keypair per identity instance — never
     * regenerated per call. HV-21: restored from disk when persistence is
     * available, so it is also stable across process restarts.
     */
    private val x25519KeyPair: KeyPair by lazy {
        val loaded = persisted?.load()
        if (loaded != null) {
            iriscore.util.IrisLog.d("x25519", "restored persisted keypair")
            loaded
        } else {
            iriscore.util.IrisLog.d("x25519", "generating fresh keypair (persisted=${persisted != null})")
            supplier.generateKeyPair().also { kp ->
                runCatching { persisted?.store(kp) }
                    .onFailure { iriscore.util.IrisLog.w("x25519", "persist store failed", it) }
                    .onSuccess { if (persisted != null) iriscore.util.IrisLog.d("x25519", "stored keypair to disk") }
            }
        }
    }

    /**
     * The one stable X25519 keypair for this identity — persisted when a
     * [Context] was supplied. [X25519KeyProviderImpl] MUST use this (not its
     * own fresh generate) so the key the engine agrees on is the same key the
     * static ad signs and that survives a restart.
     */
    fun keyPair(): KeyPair = x25519KeyPair

    fun build(): StaticAdvertisement {
        val x25519Raw = PeerIdCodec.rawPoint(x25519KeyPair.public)
        val identityRaw = ed25519.publicKeyRaw()
        val signed = bindPayload(x25519Raw, identityRaw)
        val signature = ed25519.sign(signed)
        return StaticAdvertisement(
            ed25519PubRaw = identityRaw,
            x25519PubRaw = x25519Raw,
            signature = signature,
        )
    }

    /**
     * Verify the static ad binding against the given Ed25519 public key
     * (independent verifier — no reference to the signing instance).
     */
    fun verify(
        publicKey: PublicKey,
        ad: StaticAdvertisement,
    ): Boolean {
        val signed = bindPayload(ad.x25519PubRaw, ad.ed25519PubRaw)
        return ed25519.verifyFrom(publicKey, signed, ad.signature)
    }

    /**
     * HV-21: device-scoped persistence for the X25519 key-agreement keypair,
     * mirroring [KeystoreEd25519]'s `PersistedIdentityStore` byte-for-byte: the
     * PKCS#8 private key is wrapped under a non-exportable AndroidKeyStore
     * AES-256-GCM key (`iris_x25519_wrap`) and stored with the X.509 public key
     * in `iris_x25519_static.bin` under `getNoBackupFilesDir()`. A
     * corrupt/unwrappable store rotates the key (returns null → caller
     * generates fresh) rather than crashing — the identity Ed25519 key is the
     * real trust anchor; a rotated X25519 key just costs one re-`/addkey`.
     */
    internal class PersistedX25519Store(private val context: Context) {

        private val file: File = File(context.noBackupFilesDir, KEY_FILE)

        private val keyStore: KeyStore by lazy {
            KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        }

        private val wrappingKey: SecretKey by lazy {
            if (!keyStore.containsAlias(WRAP_ALIAS)) {
                val generator = KeyGenerator.getInstance(
                    KeyProperties.KEY_ALGORITHM_AES,
                    "AndroidKeyStore",
                )
                generator.init(
                    KeyGenParameterSpec.Builder(
                        WRAP_ALIAS,
                        KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                    )
                        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setKeySize(256)
                        .build(),
                )
                generator.generateKey()
            }
            keyStore.getKey(WRAP_ALIAS, null) as SecretKey
        }

        fun load(): KeyPair? {
            iriscore.util.IrisLog.d("x25519", "load: file=$file exists=${file.exists()}")
            if (!file.exists()) return null
            return try {
                val input = DataInputStream(ByteArrayInputStream(file.readBytes()))
                if (input.readByte() != FILE_VERSION) return null
                val iv = ByteArray(input.readInt()).also { input.readFully(it) }
                val wrapped = ByteArray(input.readInt()).also { input.readFully(it) }
                val public = ByteArray(input.readInt()).also { input.readFully(it) }
                val factory = x25519KeyFactory()
                KeyPair(
                    factory.generatePublic(X509EncodedKeySpec(public)),
                    factory.generatePrivate(PKCS8EncodedKeySpec(unwrap(iv, wrapped))),
                )
            } catch (e: Exception) {
                // Rotate rather than crash — see the class doc.
                iriscore.util.IrisLog.w("x25519", "load failed — rotating key", e)
                runCatching { file.delete() }
                null
            }
        }

        fun store(keyPair: KeyPair) {
            val (iv, wrapped) = wrap(keyPair.private.encoded)
            val public = keyPair.public.encoded
            DataOutputStream(file.outputStream()).use { out ->
                out.writeByte(FILE_VERSION.toInt())
                out.writeInt(iv.size); out.write(iv)
                out.writeInt(wrapped.size); out.write(wrapped)
                out.writeInt(public.size); out.write(public)
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

        private fun x25519KeyFactory(): KeyFactory =
            runCatching { KeyFactory.getInstance("X25519") }
                .getOrElse {
                    Ed25519Provider.ensureRegistered()
                    KeyFactory.getInstance("X25519", Ed25519Provider.NAME)
                }

        companion object {
            const val KEY_FILE = "iris_x25519_static.bin"
            const val WRAP_ALIAS = "iris_x25519_wrap"
            const val FILE_VERSION: Byte = 1
        }
    }

    companion object {
        const val VERSION = 1

        /** Binding payload = X25519 pubkey followed by the Ed25519 identity pubkey. */
        fun bindPayload(x25519Raw: ByteArray, identityRaw: ByteArray): ByteArray =
            ByteArrayOutputStream(x25519Raw.size + identityRaw.size).apply {
                write(x25519Raw)
                write(identityRaw)
            }.toByteArray()
    }
}

/** Package-private seam so [X25519StaticAd.verify] verifies with an explicit public key. */
internal fun KeystoreEd25519.verifyFrom(publicKey: PublicKey, data: ByteArray, signature: ByteArray): Boolean =
    java.security.Signature.getInstance("Ed25519").run {
        initVerify(publicKey)
        update(data)
        verify(signature)
    }
