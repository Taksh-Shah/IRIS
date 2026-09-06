package iriscore.data

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import org.json.JSONObject
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

/**
 * HV-21 / HV-34: durable "known peers" map — `nodeIdHex -> x25519PubHex`.
 *
 * IRIS does no OS-level bonding (PRIVACY_MODEL.md). The durable pairing is this
 * app-level list: a peer whose X25519 key you have `/addkey`-ed stays trusted
 * across app restarts, and [MeshRepository.startMesh] re-feeds every entry into
 * the freshly-built engine key directory. Before this, `/addkey` only touched
 * the in-memory engine directory, so every cold start silently dropped every
 * trusted key and addressed messages stopped decrypting until re-keyed by hand.
 *
 * Same file (`filesDir/iris_known_peers.json`) the androidTest `IrisSnippet`
 * uses, so the app and the bench harness agree on one persisted set.
 */
@Singleton
class KnownPeersStore @Inject constructor(
    @ApplicationContext context: Context,
) {
    private val file = File(context.filesDir, FILE_NAME)

    @Synchronized
    fun all(): Map<String, String> {
        if (!file.exists()) return emptyMap()
        val o = runCatching { JSONObject(file.readText()) }.getOrElse { return emptyMap() }
        return buildMap { o.keys().forEach { put(it, o.getString(it)) } }
    }

    @Synchronized
    fun put(peerIdHex: String, x25519PubHex: String) {
        val o = if (file.exists()) {
            runCatching { JSONObject(file.readText()) }.getOrDefault(JSONObject())
        } else JSONObject()
        o.put(peerIdHex.lowercase(), x25519PubHex.lowercase())
        file.writeText(o.toString())
    }

    private companion object {
        const val FILE_NAME = "iris_known_peers.json"
    }
}
