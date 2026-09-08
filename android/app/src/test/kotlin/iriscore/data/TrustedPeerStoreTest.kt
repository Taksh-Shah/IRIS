package iriscore.data

import com.google.common.truth.Truth.assertThat
import org.junit.jupiter.api.Test
import org.junit.jupiter.api.io.TempDir
import java.io.File

class TrustedPeerStoreTest {
    @TempDir lateinit var directory: File

    @Test
    fun `a replacement key never inherits verified trust`() {
        val file = File(directory, "trusted.json")
        val store = TrustedPeerStore(file)
        val peer = "11".repeat(32)
        store.put(peer, "22".repeat(32), 0, 0, "33".repeat(64), 1, verified = true)
        store.put(peer, "44".repeat(32), 1, 0, "55".repeat(64), 2, verified = false)

        val reloaded = TrustedPeerStore(file).get(peer)
        assertThat(reloaded?.x25519Hex).isEqualTo("44".repeat(32))
        assertThat(reloaded?.verified).isFalse()
    }

    @Test
    fun `corrupt records do not erase valid siblings`() {
        val file = File(directory, "trusted.json")
        file.writeText("""{"bad":{},"${"11".repeat(32)}":{"x25519":"${"22".repeat(32)}","counter":0,"validUntil":0,"sig":"${"33".repeat(64)}","verified":true,"createdAtMs":1}}""")
        assertThat(TrustedPeerStore(file).all()).hasSize(1)
    }
}
