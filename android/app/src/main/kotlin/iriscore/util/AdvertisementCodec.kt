package iriscore.util

import iriscode.FfiPeerAdvertisement
import java.nio.ByteBuffer

/**
 * Wire encoding for the trusted-peers pairing code — a single hex blob a
 * human can paste (`/addkey <blob>`), and later embed in a QR code. This
 * encoding is Kotlin-to-Kotlin only (phone A's `/myadvert` output decoded by
 * phone B's `/addkey`) — it never crosses the Rust FFI boundary in blob form,
 * so it only needs to round-trip with itself, not match any Rust wire
 * format. Fixed layout, no framing needed since every field is a fixed size:
 * `identityPubkey(32) ++ x25519Pubkey(32) ++ keyGenCounter(8, big-endian) ++
 * validUntil(8, big-endian) ++ sig(64)` = 144 bytes = 288 hex chars.
 */
object AdvertisementCodec {

    private const val IDENTITY_LEN = 32
    private const val X25519_LEN = 32
    private const val SIG_LEN = 64
    const val TOTAL_LEN = IDENTITY_LEN + X25519_LEN + 8 + 8 + SIG_LEN

    fun encode(ad: FfiPeerAdvertisement): String {
        val buf = ByteBuffer.allocate(TOTAL_LEN)
        buf.put(ad.identityPubkey)
        buf.put(ad.x25519Pubkey)
        buf.putLong(ad.keyGenCounter.toLong())
        buf.putLong(ad.validUntil.toLong())
        buf.put(ad.sig)
        return PeerIdCodec.toHex(buf.array())
    }

    /** `null` if `blob` isn't a valid hex string of exactly [TOTAL_LEN] bytes. */
    fun decode(blob: String): FfiPeerAdvertisement? {
        val bytes = PeerIdCodec.fromHex(blob.trim()) ?: return null
        if (bytes.size != TOTAL_LEN) return null
        val buf = ByteBuffer.wrap(bytes)
        val identity = ByteArray(IDENTITY_LEN).also { buf.get(it) }
        val x25519 = ByteArray(X25519_LEN).also { buf.get(it) }
        val counter = buf.getLong().toULong()
        val validUntil = buf.getLong().toULong()
        val sig = ByteArray(SIG_LEN).also { buf.get(it) }
        return FfiPeerAdvertisement(
            identityPubkey = identity,
            x25519Pubkey = x25519,
            keyGenCounter = counter,
            validUntil = validUntil,
            sig = sig,
        )
    }
}
