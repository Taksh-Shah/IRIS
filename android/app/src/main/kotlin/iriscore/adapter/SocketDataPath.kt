package iriscore.adapter

import java.io.Closeable
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.IOException
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.ConcurrentHashMap
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Shared TCP data path for the Wi-Fi Aware (NDP) and Wi-Fi Direct (TCP-over-GO)
 * transports.
 *
 * Both adapters previously enqueued outbound frames into a ring buffer and then
 * drained *that same buffer* on the inbound path, so every frame a node sent
 * was handed back to the core as if a peer had sent it. On real hardware that
 * reads as a working link carrying traffic that never left the device. The
 * inbound and outbound buffers are now strictly separate, and real frames move
 * over the sockets established here.
 *
 * Wire format is a 4-byte big-endian length followed by exactly that many
 * payload bytes. The core's CRYPTO-001 envelope is carried verbatim as the
 * payload — this framing exists only to recover message boundaries from the
 * TCP byte stream and is never interpreted.
 */
internal object FrameCodec {

    /**
     * Upper bound on a single frame. Both links carry attacker-reachable bytes,
     * and the length prefix is read before any authentication happens, so an
     * unbounded prefix is a one-packet remote OOM. Sized well above the largest
     * legitimate envelope.
     */
    const val MAX_FRAME_BYTES: Int = 1 shl 20 // 1 MiB
}

/**
 * A connected, length-prefixed frame link over one TCP socket.
 *
 * Reads run on a dedicated IO coroutine and push complete frames to [onFrame].
 * Writes are serialised by a mutex so concurrent `send` calls cannot interleave
 * two frames on the wire.
 */
internal class FramedSocketLink(
    private val socket: Socket,
    scope: CoroutineScope,
    private val onFrame: (ByteArray) -> Unit,
    private val onClosed: (Throwable?) -> Unit = {},
) : Closeable {

    private val output = DataOutputStream(socket.getOutputStream().buffered())
    private val input = DataInputStream(socket.getInputStream().buffered())
    private val writeMutex = Mutex()

    @Volatile
    private var closed = false

    private val readJob: Job = scope.launch(Dispatchers.IO) { readLoop() }

    /** @return true when the frame reached the socket. */
    suspend fun send(payload: ByteArray): Boolean {
        if (closed || payload.size > FrameCodec.MAX_FRAME_BYTES) return false
        return try {
            writeMutex.withLock {
                output.writeInt(payload.size)
                output.write(payload)
                output.flush()
            }
            true
        } catch (_: IOException) {
            closeQuietly(null)
            false
        }
    }

    private fun readLoop() {
        try {
            while (!closed) {
                val length = input.readInt()
                // A hostile or desynchronised peer can claim any length; reject
                // out-of-range prefixes instead of trying to allocate them.
                if (length < 0 || length > FrameCodec.MAX_FRAME_BYTES) {
                    closeQuietly(IOException("frame length $length out of range"))
                    return
                }
                val payload = ByteArray(length)
                input.readFully(payload)
                // Re-check before delivering: `close()` cannot interrupt a
                // blocking `readFully`, so a frame already buffered when the
                // link was torn down would otherwise be handed to `onFrame`
                // after teardown and enqueued into an inbox nobody drains.
                if (closed) return
                onFrame(payload)
            }
        } catch (e: IOException) {
            closeQuietly(e)
        } catch (e: RuntimeException) {
            closeQuietly(e)
        }
    }

    private fun closeQuietly(cause: Throwable?) {
        if (closed) return
        closed = true
        runCatching { socket.close() }
        onClosed(cause)
    }

    val isOpen: Boolean get() = !closed && !socket.isClosed

    override fun close() {
        if (closed) return
        closed = true
        readJob.cancel()
        // Close the socket first: the read coroutine is parked in a blocking
        // `readFully` that cancellation cannot interrupt, so closing the fd is
        // what actually unblocks it.
        runCatching { socket.close() }
        // Notify on this path too. Only `closeQuietly` used to, so the
        // registry kept a dead entry whenever teardown went through `close()`.
        onClosed(null)
    }
}

/**
 * Handle -> live link table. A handle without a link is not an error: frames for
 * it are held in the adapter's outbox until the socket comes up, then flushed.
 */
internal class SocketLinkRegistry : Closeable {

    private val links = ConcurrentHashMap<Long, FramedSocketLink>()

    fun put(handle: Long, link: FramedSocketLink) {
        links.put(handle, link)?.close()
    }

    fun get(handle: Long): FramedSocketLink? = links[handle]?.takeIf { it.isOpen }

    fun remove(handle: Long) {
        links.remove(handle)?.close()
    }

    fun handles(): Set<Long> = links.keys.toSet()

    override fun close() {
        links.values.forEach { it.close() }
        links.clear()
    }
}

/**
 * Accepts inbound connections for the responder half of a link.
 *
 * Bound to port 0 unless a fixed port is given, so the OS assigns a free one;
 * [port] is then advertised to the peer (the Wi-Fi Aware network specifier for
 * the responder, or the well-known Wi-Fi Direct GO port).
 */
internal class FramedSocketServer(
    private val serverSocket: ServerSocket,
    private val scope: CoroutineScope,
    /**
     * Receives each accepted socket. The adapter — not the server — builds the
     * [FramedSocketLink], because only it knows which peer handle the
     * connection maps to and therefore where inbound frames belong.
     */
    private val onAccepted: (Socket) -> Unit,
) : Closeable {

    val port: Int get() = serverSocket.localPort

    @Volatile
    private var closed = false

    /** Currently-accepted links, bounded by [MAX_CONCURRENT_LINKS]. */
    private val live = java.util.concurrent.atomic.AtomicInteger(0)

    private val acceptJob: Job = scope.launch(Dispatchers.IO) {
        while (!closed) {
            val socket = try {
                serverSocket.accept()
            } catch (_: IOException) {
                return@launch // socket closed, or the radio went away
            }
            // Bounded: an in-range attacker could otherwise open sockets until
            // the IO dispatcher and the process fd limit were exhausted.
            if (live.get() >= MAX_CONCURRENT_LINKS) {
                runCatching { socket.close() }
                continue
            }
            live.incrementAndGet()
            // `onAccepted` runs adapter code. A throw used to escape the
            // `while`, ending the loop with `closed == false` and the socket
            // still bound — the group owner silently stopped accepting for the
            // rest of the process, with no error anywhere.
            try {
                onAccepted(socket)
            } catch (_: Throwable) {
                live.decrementAndGet()
                runCatching { socket.close() }
            }
        }
    }

    /** Releases one slot from the concurrent-link budget. */
    fun releaseSlot() {
        live.updateAndGet { if (it > 0) it - 1 else 0 }
    }

    override fun close() {
        closed = true
        acceptJob.cancel()
        runCatching { serverSocket.close() }
    }

    companion object {
        /**
         * Well-known Wi-Fi Direct group-owner port. Wi-Fi Direct gives clients
         * the GO's address but no port-negotiation channel, so both ends must
         * agree on it ahead of time.
         */
        const val WIFI_DIRECT_GO_PORT = 47_631

        /**
         * Ceiling on simultaneously-accepted links. A mesh group owner serves a
         * handful of clients; anything beyond this is abuse.
         */
        const val MAX_CONCURRENT_LINKS = 16
    }
}
