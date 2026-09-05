package iriscore.snippet

import androidx.test.platform.app.InstrumentationRegistry
import com.google.android.mobly.snippet.Snippet
import com.google.android.mobly.snippet.rpc.Rpc
import iriscode.FfiInboxListener
import iriscode.FfiIncomingMessage
import iriscode.IrisEngine
import iriscore.adapter.AndroidBleTransportAdapter
import iriscore.adapter.AndroidWifiAwareTransportAdapter
import iriscore.adapter.AndroidWifiDirectTransportAdapter
import iriscore.identity.KeystoreEd25519
import iriscore.identity.X25519KeyProviderImpl
import iriscore.identity.X25519StaticAd
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.concurrent.ConcurrentHashMap

/**
 * HV-2 — the `iris_bench` device-side RPC surface (Mobly Snippet Lib).
 *
 * Each bench phone loads this over `adb shell am instrument`; the host-side
 * Python (`iris_bench`) drives both phones at once. The snippet builds a real
 * `IrisEngine` over the real Android transport adapters — the same objects the
 * app's Hilt graph wires — so a test exercises the production radio path, not a
 * simulator.
 *
 * All returns are JSON strings so the Python side never has to guess types.
 */
class IrisSnippet : Snippet {

    private val context by lazy {
        InstrumentationRegistry.getInstrumentation().targetContext
    }
    private val keystore by lazy { KeystoreEd25519(context) }
    private val x25519Provider by lazy { X25519KeyProviderImpl(X25519StaticAd(keystore)) }

    /**
     * HV-34: persisted "known peers" — nodeIdHex -> x25519PubHex. IRIS does no
     * OS-level BLE bonding (research: bonding does not speed up reconnects, and
     * the raw MAC is deliberately not identity — see PRIVACY_MODEL.md). The
     * durable pairing is instead this app-level list: two devices that have
     * exchanged identities stay "friends" across app restarts / reinstalls, and
     * `startMesh` re-feeds every entry into the engine key directory so a fresh
     * engine can seal addressed mail to them without the harness re-registering
     * each run. (The real app's contacts UI is HV-56.)
     */
    private val knownPeersFile by lazy { File(context.filesDir, "iris_known_peers.json") }

    @Synchronized
    private fun loadKnownPeers(): JSONObject =
        if (knownPeersFile.exists()) {
            runCatching { JSONObject(knownPeersFile.readText()) }.getOrDefault(JSONObject())
        } else JSONObject()

    @Synchronized
    private fun persistKnownPeer(peerIdHex: String, x25519PubHex: String) {
        val o = loadKnownPeers().put(peerIdHex.lowercase(), x25519PubHex.lowercase())
        knownPeersFile.writeText(o.toString())
    }

    @Volatile private var engine: IrisEngine? = null

    /** wire-message-id hex -> {latencyMs, priority, senderHex} of the delivered copy. */
    private val delivered = ConcurrentHashMap<String, JSONObject>()

    /** wire-message-id hex -> epoch millis the send was accepted (for latency). */
    private val sentAt = ConcurrentHashMap<String, Long>()

    private fun ByteArray.hex(): String =
        joinToString("") { "%02x".format(it) }

    // ---- lifecycle -----------------------------------------------------------

    @Rpc(description = "Build the engine over the real adapters, install the inbox listener, start all transports.")
    fun startMesh(): String {
        if (engine != null) return snapshotJson()
        val nodeId = keystore.publicKeyRaw()
        val signer = object : iriscode.FfiCryptoSigner {
            override fun sign(data: ByteArray): ByteArray = keystore.sign(data)
        }
        val e = IrisEngine.newWithX25519(
            AndroidBleTransportAdapter(context),
            AndroidWifiAwareTransportAdapter(context),
            AndroidWifiDirectTransportAdapter(context),
            nodeId,
            signer,
            x25519Provider,
        )
        // HV-21: same wiring as the production Hilt module (IrisCoreModule) —
        // the snippet builds its own engine instance, so it needs its own
        // registration.
        iriscore.util.LocalWifiDirectAddress.onChanged = { mac ->
            runCatching { e.setLocalWifiDirectMac(mac) }
        }
        iriscore.util.LocalWifiDirectAddress.address?.let { mac ->
            runCatching { e.setLocalWifiDirectMac(mac) }
        }
        e.subscribeInbox(object : FfiInboxListener {
            override fun onMessage(message: FfiIncomingMessage) {
                val idHex = message.messageId.hex()
                val started = sentAt[idHex]
                delivered[idHex] = JSONObject().apply {
                    put("delivered", true)
                    put("latencyMs", if (started != null) System.currentTimeMillis() - started else -1L)
                    put("priority", message.priority.toInt())
                    put("senderHex", message.senderId.hex())
                    put("payloadUtf8", String(message.payload, Charsets.UTF_8))
                }
            }
        })
        // HV-34: re-feed every persisted known peer so a fresh engine can seal
        // addressed mail to a "friend" without the harness re-registering.
        val known = loadKnownPeers()
        for (peerId in known.keys()) {
            runCatching { e.registerPeerKey(peerId, known.getString(peerId)) }
        }
        e.startAll()
        engine = e
        return snapshotJson()
    }

    @Rpc(description = "HV-97: drop every live link (advertising/scanning stay up) so discovery re-forms them — the deterministic reconnect trigger for HV-14/HV-15.")
    fun dropAllLinks() {
        (engine ?: error("startMesh first")).dropAllLinks()
    }

    @Rpc(description = "Stop all transports and drop the engine.")
    fun stopMesh() {
        engine?.stopAll()
        engine = null
        delivered.clear()
        sentAt.clear()
    }

    // ---- identity ----------------------------------------------------------

    @Rpc(description = "This node's 64-hex PeerId.")
    fun nodeId(): String = keystore.publicKeyRaw().hex()

    @Rpc(description = "This node's 64-hex X25519 static public key (what a sender must encrypt to).")
    fun staticX25519(): String = x25519Provider.staticPublicKey().hex()

    @Rpc(description = "Register a peer's X25519 static public key so addressed sends to it can be sealed (HV-89 interim). Also persists it as a known peer (HV-34).")
    fun registerPeerKey(peerIdHex: String, x25519PubHex: String) {
        (engine ?: error("startMesh first")).registerPeerKey(peerIdHex, x25519PubHex)
        persistKnownPeer(peerIdHex, x25519PubHex)
    }

    @Rpc(description = "HV-34: persist a peer as a permanent 'friend' (nodeId + X25519 key). Survives app restart/reinstall; startMesh re-feeds it to the engine. Works before startMesh.")
    fun addFriend(peerIdHex: String, x25519PubHex: String) {
        persistKnownPeer(peerIdHex, x25519PubHex)
        engine?.let { runCatching { it.registerPeerKey(peerIdHex, x25519PubHex) } }
    }

    @Rpc(description = "HV-34: the persisted known-peer map (nodeIdHex -> x25519Hex) as a JSON string.")
    fun knownPeers(): String = loadKnownPeers().toString()

    @Rpc(description = "HV-34: forget all persisted known peers (test hygiene).")
    fun clearFriends() {
        knownPeersFile.delete()
    }

    // ---- messaging -------------------------------------------------------

    @Rpc(description = "Send text to a 64-hex PeerId at the given priority (0..7). Returns the 32-hex wire message id.")
    fun sendText(peerHex: String, text: String, priority: Int): String {
        val e = engine ?: error("startMesh first")
        val id = e.sendText(peerHex, text, priority.toUByte()).hex()
        sentAt[id] = System.currentTimeMillis()
        return id
    }

    @Rpc(description = "Block until the given wire message id is delivered to this node, or timeout. Returns {delivered, latencyMs, ...}.")
    fun awaitDelivered(messageIdHex: String, timeoutMs: Long): String {
        val deadline = System.currentTimeMillis() + timeoutMs
        while (System.currentTimeMillis() < deadline) {
            delivered[messageIdHex]?.let { return it.toString() }
            Thread.sleep(100)
        }
        return JSONObject().put("delivered", false).put("timeoutMs", timeoutMs).toString()
    }

    // ---- diagnostics -----------------------------------------------------

    @Rpc(description = "The full mesh diagnostic (transports, neighbours, counters, recent events) as JSON.")
    fun meshSnapshot(): String = snapshotJson()

    private fun snapshotJson(): String {
        val e = engine ?: return JSONObject().put("running", false).toString()
        val s = e.snapshot()
        return JSONObject().apply {
            put("running", true)
            put("nodeIdHex", s.nodeIdHex)
            put("transports", JSONArray().apply {
                s.transports.forEach {
                    put(JSONObject().apply {
                        put("id", it.id)
                        put("state", it.state)
                        put("batteryMa", it.estimatedBatteryMa.toDouble())
                    })
                }
            })
            put("neighbors", JSONArray().apply {
                s.neighbors.forEach {
                    put(JSONObject().apply {
                        put("peerIdHex", it.peerIdHex)
                        put("state", it.state)
                        put("links", JSONArray(it.links))
                    })
                }
            })
            put("messages", JSONObject().apply {
                put("sent", s.messages.sent.toLong())
                put("delivered", s.messages.delivered.toLong())
                put("relayed", s.messages.relayed.toLong())
                put("expired", s.messages.expired.toLong())
                put("deliveryFailed", s.messages.deliveryFailed.toLong())
            })
            put("counters", JSONObject().apply {
                s.counters.filter { it.value > 0uL }.forEach { put(it.name, it.value.toLong()) }
            })
            put("recentEvents", JSONArray().apply {
                s.recentEvents.takeLast(40).forEach {
                    put(JSONObject().apply {
                        put("level", it.level)
                        put("event", it.event)
                        put("detail", it.detail)
                    })
                }
            })
        }.toString()
    }

    override fun shutdown() {
        try {
            stopMesh()
        } catch (_: Throwable) {
        }
    }
}
