package iriscore.identity

import iriscode.FfiX25519KeyProvider
import iriscore.util.PeerIdCodec
import javax.crypto.KeyAgreement

/**
 * AN-6 — wires [X25519StaticAd] into the Rust engine as an [FfiX25519KeyProvider].
 *
 * The static X25519 keypair is generated once (lazy) and reused for the engine
 * lifetime, consistent with the [X25519StaticAd.x25519KeyPair] laziness contract.
 * DH is performed entirely inside JCA so the raw secret scalar never crosses the
 * FFI boundary — the Rust engine only receives the 32-byte shared secret.
 */
class X25519KeyProviderImpl(
    private val ad: X25519StaticAd,
) : FfiX25519KeyProvider {

    /**
     * The stable X25519 keypair — THE SAME instance [X25519StaticAd] signs into
     * the static ad, and (HV-21) the one persisted to disk when the ad was
     * built with a Context. This used to be a fresh `KeyPairGenerator` generate
     * here that ignored `ad` entirely, so the engine's key rotated every
     * process start and every peer's `/addkey`/friend entry silently went
     * stale on the next cold start.
     */
    private val keyPair get() = ad.keyPair()

    /** 32-byte X25519 public key advertised in the static-ad binding. */
    override fun staticPublicKey(): ByteArray =
        PeerIdCodec.rawPoint(keyPair.public)

    /**
     * Compute X25519(static_secret, ephemeralPubkey) entirely inside JCA.
     * Returns the 32-byte raw shared secret; the static private key never leaves.
     */
    override fun diffieHellman(ephemeralPubkey: ByteArray): ByteArray {
        require(ephemeralPubkey.size == 32) { "X25519 ephemeral pubkey must be 32 bytes" }
        // Reconstruct the peer public key from raw bytes via the PKCS8/X509 envelope.
        val pubKeySpec = java.security.spec.X509EncodedKeySpec(encodeX25519Public(ephemeralPubkey))
        val peerPublic = java.security.KeyFactory.getInstance("X25519")
            .generatePublic(pubKeySpec)
        val ka = KeyAgreement.getInstance("X25519")
        ka.init(keyPair.private)
        ka.doPhase(peerPublic, true)
        return ka.generateSecret()
    }

    companion object {
        /**
         * Encode 32 raw X25519 bytes into the DER X.509 SubjectPublicKeyInfo
         * structure that JCA's `KeyFactory.getInstance("X25519")` accepts.
         *
         * Structure (per RFC 8410):
         *   SEQUENCE {
         *     SEQUENCE { OID 1.3.101.110 }   -- id-X25519
         *     BIT STRING { 0x00 || rawKey }
         *   }
         */
        private val X25519_OID_PREFIX = byteArrayOf(
            0x30, 0x2a,             // SEQUENCE (42 bytes)
            0x30, 0x05,             // SEQUENCE (5 bytes)
            0x06, 0x03,             // OID (3 bytes)
            0x2b, 0x65, 0x6e,       // 1.3.101.110 (id-X25519)
            0x03, 0x21,             // BIT STRING (33 bytes)
            0x00,                   // leading zero (unused bits)
        )

        private fun encodeX25519Public(raw: ByteArray): ByteArray =
            X25519_OID_PREFIX + raw
    }
}
