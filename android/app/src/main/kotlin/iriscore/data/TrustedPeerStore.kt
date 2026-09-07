package iriscore.data

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import org.json.JSONObject
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Durable trusted-peer record: the signed key advertisement a peer produced
 * with their own Keystore-backed Ed25519 key (scanned via QR or pasted via
 * `/addkey`), plus whether the operator has out-of-band confirmed it
 * (`/fingerprint <name> confirm`).
 *
 * This is the persistence half of the trusted-peers feature — the Rust
 * `TrustStore` (`crates/iris-core/src/identity/trust_store.rs`) that actually
 * enforces trust decisions is in-memory only and rebuilt empty on every
 * process start. [MeshRepository.startMesh] replays every entry here back
 * into the fresh engine (`adoptPeerAdvertisement` + `verifyPeer` for already-
 * verified peers) before transports come up — the same re-feed pattern
 * [KnownPeersStore] already established for the older bare-key path.
 */
data class TrustedPeerRecord(
    val x25519Hex: String,
    val keyGenCounter: Long,
    val validUntil: Long,
    val sigHex: String,
    val verified: Boolean,
    val createdAtMs: Long,
)

@Singleton
class TrustedPeerStore @Inject constructor(
    @ApplicationContext context: Context,
) {
    private val file = File(context.noBackupFilesDir, FILE_NAME)

    @Synchronized
    fun all(): Map<String, TrustedPeerRecord> {
        if (!file.exists()) return emptyMap()
        val o = runCatching { JSONObject(file.readText()) }.getOrElse { return emptyMap() }
        return buildMap {
            o.keys().forEach { peerHex ->
                val entry = o.getJSONObject(peerHex)
                put(
                    peerHex,
                    TrustedPeerRecord(
                        x25519Hex = entry.getString("x25519"),
                        keyGenCounter = entry.optLong("counter", 0L),
                        validUntil = entry.optLong("validUntil", 0L),
                        sigHex = entry.getString("sig"),
                        verified = entry.optBoolean("verified", false),
                        createdAtMs = entry.optLong("createdAtMs", 0L),
                    ),
                )
            }
        }
    }

    @Synchronized
    fun get(peerIdHex: String): TrustedPeerRecord? = all()[peerIdHex.lowercase()]

    /** Insert or overwrite a peer's advertisement. Does not touch `verified`. */
    @Synchronized
    fun put(
        peerIdHex: String,
        x25519Hex: String,
        keyGenCounter: Long,
        validUntil: Long,
        sigHex: String,
        createdAtMs: Long,
    ) {
        val root = readRoot()
        val entry = JSONObject()
        entry.put("x25519", x25519Hex.lowercase())
        entry.put("counter", keyGenCounter)
        entry.put("validUntil", validUntil)
        entry.put("sig", sigHex.lowercase())
        entry.put("verified", root.optJSONObject(peerIdHex.lowercase())?.optBoolean("verified", false) ?: false)
        entry.put("createdAtMs", createdAtMs)
        root.put(peerIdHex.lowercase(), entry)
        file.writeText(root.toString())
    }

    /** Mark a peer `Verified` (out-of-band SAS confirmation succeeded). */
    @Synchronized
    fun markVerified(peerIdHex: String) {
        val root = readRoot()
        val entry = root.optJSONObject(peerIdHex.lowercase()) ?: return
        entry.put("verified", true)
        root.put(peerIdHex.lowercase(), entry)
        file.writeText(root.toString())
    }

    @Synchronized
    fun remove(peerIdHex: String) {
        val root = readRoot()
        root.remove(peerIdHex.lowercase())
        file.writeText(root.toString())
    }

    private fun readRoot(): JSONObject =
        if (file.exists()) {
            runCatching { JSONObject(file.readText()) }.getOrDefault(JSONObject())
        } else {
            JSONObject()
        }

    private companion object {
        const val FILE_NAME = "iris_trusted_peers.json"
    }
}
