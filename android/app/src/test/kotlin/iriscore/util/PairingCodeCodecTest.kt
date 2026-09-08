package iriscore.util

import com.google.common.truth.Truth.assertThat
import iriscode.FfiPeerAdvertisement
import org.junit.jupiter.api.Test

class PairingCodeCodecTest {
    private val advertisement = FfiPeerAdvertisement(
        identityPubkey = ByteArray(32) { it.toByte() },
        x25519Pubkey = ByteArray(32) { (it + 32).toByte() },
        keyGenCounter = 7uL,
        validUntil = 99uL,
        sig = ByteArray(64) { (it + 64).toByte() },
    )

    @Test
    fun `qr envelope round trips the canonical signed advertisement`() {
        val canonical = AdvertisementCodec.encode(advertisement)
        val qr = PairingCodeCodec.forQr(canonical)

        assertThat(qr).startsWith(PairingCodeCodec.PREFIX)
        assertThat(PairingCodeCodec.toAdvertisementHex(qr!!)).isEqualTo(canonical)
    }

    @Test
    fun `legacy canonical hex remains a valid manual fallback`() {
        val canonical = AdvertisementCodec.encode(advertisement)
        assertThat(PairingCodeCodec.toAdvertisementHex(canonical.uppercase())).isEqualTo(canonical)
    }

    @Test
    fun `oversized malformed and wrong version inputs fail closed`() {
        assertThat(PairingCodeCodec.toAdvertisementHex("x".repeat(513))).isNull()
        assertThat(PairingCodeCodec.toAdvertisementHex("iris:pair:v2:AAAA")).isNull()
        assertThat(PairingCodeCodec.toAdvertisementHex(PairingCodeCodec.PREFIX + "not_base64!" )).isNull()
    }
}
