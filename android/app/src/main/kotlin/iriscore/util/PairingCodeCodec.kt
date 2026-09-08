package iriscore.util

import java.util.Base64

/** Versioned QR envelope around the canonical signed advertisement bytes. */
object PairingCodeCodec {
    const val PREFIX = "iris:pair:v1:"
    const val MAX_INPUT_CHARS = 512

    fun forQr(advertisementHex: String): String? {
        val ad = AdvertisementCodec.decode(advertisementHex) ?: return null
        val canonicalHex = AdvertisementCodec.encode(ad)
        val bytes = PeerIdCodec.fromHex(canonicalHex) ?: return null
        return PREFIX + Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)
    }

    /** Return canonical 288-char advertisement hex from QR or legacy paste. */
    fun toAdvertisementHex(input: String): String? {
        val value = input.trim()
        if (value.isEmpty() || value.length > MAX_INPUT_CHARS) return null
        if (!value.startsWith(PREFIX)) {
            return value.lowercase().takeIf { AdvertisementCodec.decode(it) != null }
        }
        val encoded = value.removePrefix(PREFIX)
        val bytes = runCatching { Base64.getUrlDecoder().decode(encoded) }.getOrNull() ?: return null
        if (bytes.size != AdvertisementCodec.TOTAL_LEN) return null
        val hex = PeerIdCodec.toHex(bytes)
        return hex.takeIf { AdvertisementCodec.decode(it) != null }
    }
}
