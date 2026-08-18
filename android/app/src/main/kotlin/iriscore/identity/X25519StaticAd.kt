package iriscore.identity

import iriscore.util.PeerIdCodec
import java.io.ByteArrayOutputStream
import java.security.KeyPair
import java.security.KeyPairGenerator
import java.security.PublicKey

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
 * JCA `X25519`, JDK 11+/Conscrypt); Keystore-managed X25519 (API 33+) is a
 * future hardening (recorded, not a blocker). Pure JVM — unit-tested on the
 * host Gradle `test` leg.
 */
class X25519StaticAd(
    private val ed25519: KeystoreEd25519,
    private val supplier: X25519KeySupplier = JcaX25519KeySupplier(),
) {

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

    /** AND-RT-107: one stable X25519 keypair per identity instance — never regenerated per call. */
    private val x25519KeyPair: KeyPair by lazy { supplier.generateKeyPair() }

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