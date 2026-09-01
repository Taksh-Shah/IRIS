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
        e.startAll()
        engine = e
        return snapshotJson()
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

    @Rpc(description = "Register a peer's X25519 static public key so addressed sends to it can be sealed (HV-89 interim).")
    fun registerPeerKey(peerIdHex: String, x25519PubHex: String) {
        (engine ?: error("startMesh first")).registerPeerKey(peerIdHex, x25519PubHex)
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
