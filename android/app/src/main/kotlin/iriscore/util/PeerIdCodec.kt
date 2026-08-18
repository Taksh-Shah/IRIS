package iriscore.util

/**
 * Hex codec for the 32-byte / 64-hex PeerId domain (matches the Rust identity
 * module: raw Ed25519 public key -> 64-hex node id, 16-hex short id in UI).
 *
 * Pure JVM — unit-tested on the host Gradle `test` leg.
 */
object PeerIdCodec {

    private val HEX = "0123456789abcdef"

    fun toHex(bytes: ByteArray): String {
        val sb = StringBuilder(bytes.size * 2)
        for (b in bytes) {
            val v = b.toInt() and 0xff
            sb.append(HEX[v ushr 4]).append(HEX[v and 0x0f])
        }
        return sb.toString()
    }

    /** `null` unless `hex` is an even-length lowercase/uppercase hex string. */
    fun fromHex(hex: String): ByteArray? {
        val s = hex.trim()
        if (s.isEmpty() || s.length % 2 != 0) return null
        if (s.any { it !in "0123456789abcdefABCDEF" }) return null
        return ByteArray(s.length / 2) { i ->
            s.substring(i * 2, i * 2 + 2).toInt(16).toByte()
        }
    }

    fun isHex(hex: String): Boolean = fromHex(hex) != null

    /** First 16 hex chars of the 64-hex node id for compact UI display. */
    fun shortId(nodeIdHex: String): String = nodeIdHex.take(16)

    /**
     * Raw 32-byte point from an X.509 SPKI public key (Ed25519/X25519 share
     * the same `SEQUENCE { OID, BIT STRING(32) }` layout = 44 bytes encoded;
     * the raw point is the final 32 bytes). Handles already-raw keys too.
     */
    fun rawPoint(publicKey: java.security.PublicKey): ByteArray {
        val enc = publicKey.encoded ?: error("public key has no encoding")
        return when (enc.size) {
            32 -> enc
            44 -> enc.copyOfRange(12, 44)
            else -> error("unexpected point encoding size ${enc.size}")
        }
    }
}