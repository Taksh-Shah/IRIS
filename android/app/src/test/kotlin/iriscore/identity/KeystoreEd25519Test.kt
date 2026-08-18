package iriscore.identity

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * AC-8 unit tests — host-JVM leg. The AndroidKeyStore (TEE) path cannot run on
 * the host (no Android runtime); the tests exercise the [KeystoreEd25519.SoftwareBackend]
 * fallback end-to-end plus the RED-0005 X25519 static-ad binding. The TEE path
 * is code-reviewed + exercised on the device/CI leg.
 */
class KeystoreEd25519Test {

    private fun softwareIdentity(): KeystoreEd25519 = KeystoreEd25519(KeystoreEd25519.SoftwareBackend())

    @Test
    fun `identity backend reports SOFTWARE on the host`() {
        assertEquals(KeystoreEd25519.BackendType.SOFTWARE, softwareIdentity().backendType)
    }

    @Test
    fun `raw public key is 32 bytes and node id is 64 hex`() {
        val id = softwareIdentity()
        assertEquals(32, id.publicKeyRaw().size)
        assertEquals(64, id.nodeIdHex().length)
        assertTrue(id.nodeIdHex().all { it in "0123456789abcdef" })
    }

    @Test
    fun `node id is stable across loads - persistent provisioning`() {
        val backend = KeystoreEd25519.SoftwareBackend()
        val first = KeystoreEd25519(backend).nodeIdHex()
        val second = KeystoreEd25519(backend).nodeIdHex()
        assertEquals(first, second)
    }

    @Test
    fun `sign verify round trip`() {
        val id = softwareIdentity()
        val msg = "hello mesh".encodeToByteArray()
        val sig = id.sign(msg)
        assertTrue(id.verify(msg, sig))
    }

    @Test
    fun `signature rejects tampered message`() {
        val id = softwareIdentity()
        val msg = "hello mesh".encodeToByteArray()
        val sig = id.sign(msg)
        assertFalse(id.verify("hello mesH".encodeToByteArray(), sig))
    }

    @Test
    fun `signature rejects signature swapped across identities`() {
        val a = softwareIdentity()
        val b = softwareIdentity()
        val msg = "payload".encodeToByteArray()
        assertFalse(a.verify(msg, b.sign(msg)))
    }

    // -- X25519StaticAd (RED-0005 two-keypair binding) ----------------------

    @Test
    fun `static ad binds x25519 pubkey signed by ed25519 identity`() {
        val ed = softwareIdentity()
        val ad = X25519StaticAd(ed).build()

        assertEquals(X25519StaticAd.VERSION, ad.version)
        assertEquals(32, ad.ed25519PubRaw.size)
        assertEquals(32, ad.x25519PubRaw.size)

        // Independent verifier using only the advertised keys verifies the binding.
        assertTrue(X25519StaticAd(ed).verify(ed.publicKey, ad))
    }

    @Test
    fun `static ad binding is tamper-evident`() {
        val ed = softwareIdentity()
        val ad = X25519StaticAd(ed).build()

        val tampered = ad.copy(x25519PubRaw = ad.x25519PubRaw.copyOf().also { it[0] = (it[0] + 1).toByte() })
        assertFalse(X25519StaticAd(ed).verify(ed.publicKey, tampered))
    }
}