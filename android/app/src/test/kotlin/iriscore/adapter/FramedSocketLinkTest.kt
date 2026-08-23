package iriscore.adapter

import java.io.DataOutputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Real-socket coverage for the Wi-Fi Aware / Wi-Fi Direct data path.
 *
 * These run over a loopback TCP pair rather than a mock, so the length-prefix
 * framing, the partial-read reassembly and the hostile-length guard are all
 * exercised the way they would be on an NDP or GO link.
 */
class FramedSocketLinkTest {

    /** Connected loopback socket pair. */
    private fun socketPair(): Pair<Socket, Socket> {
        val server = ServerSocket(0)
        val client = Socket(InetAddress.getLoopbackAddress(), server.localPort)
        val accepted = server.accept()
        server.close()
        return client to accepted
    }

    private fun scope() = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    @Test
    fun `frames round-trip intact over a real socket`() = runBlocking {
        val (a, b) = socketPair()
        val received = CopyOnWriteArrayList<ByteArray>()
        val latch = CountDownLatch(2)

        val sender = FramedSocketLink(a, scope(), onFrame = {})
        val receiver = FramedSocketLink(
            b,
            scope(),
            onFrame = { received.add(it); latch.countDown() },
        )

        val first = "envelope-one".toByteArray()
        val second = ByteArray(5000) { (it % 251).toByte() }
        assertTrue(sender.send(first))
        assertTrue(sender.send(second))

        assertTrue(latch.await(5, TimeUnit.SECONDS), "frames not delivered")
        assertEquals(2, received.size)
        // Boundaries are preserved: a 5000-byte frame is not split or merged
        // even though it spans several TCP segments.
        assertArrayEquals(first, received[0])
        assertArrayEquals(second, received[1])

        sender.close()
        receiver.close()
    }

    @Test
    fun `empty frame is delivered as an empty payload`() = runBlocking {
        val (a, b) = socketPair()
        val latch = CountDownLatch(1)
        val received = CopyOnWriteArrayList<ByteArray>()

        val sender = FramedSocketLink(a, scope(), onFrame = {})
        val receiver = FramedSocketLink(
            b,
            scope(),
            onFrame = { received.add(it); latch.countDown() },
        )

        assertTrue(sender.send(ByteArray(0)))
        assertTrue(latch.await(5, TimeUnit.SECONDS))
        assertEquals(0, received.single().size)

        sender.close()
        receiver.close()
    }

    @Test
    fun `oversized outbound frame is refused rather than written`() = runBlocking {
        val (a, b) = socketPair()
        val link = FramedSocketLink(a, scope(), onFrame = {})

        assertFalse(link.send(ByteArray(FrameCodec.MAX_FRAME_BYTES + 1)))

        link.close()
        b.close()
    }

    @Test
    fun `hostile length prefix closes the link instead of allocating`() = runBlocking {
        val (a, b) = socketPair()
        val closed = CountDownLatch(1)
        val received = CopyOnWriteArrayList<ByteArray>()

        val receiver = FramedSocketLink(
            b,
            scope(),
            onFrame = { received.add(it) },
            onClosed = { closed.countDown() },
        )

        // A peer claiming a 2 GiB frame must not cause a 2 GiB allocation.
        DataOutputStream(a.getOutputStream()).apply {
            writeInt(Int.MAX_VALUE)
            flush()
        }

        assertTrue(closed.await(5, TimeUnit.SECONDS), "link stayed open on a hostile length")
        assertTrue(received.isEmpty())
        assertFalse(receiver.isOpen)

        a.close()
    }

    @Test
    fun `negative length prefix closes the link`() = runBlocking {
        val (a, b) = socketPair()
        val closed = CountDownLatch(1)

        val receiver = FramedSocketLink(b, scope(), onFrame = {}, onClosed = { closed.countDown() })

        DataOutputStream(a.getOutputStream()).apply {
            writeInt(-1)
            flush()
        }

        assertTrue(closed.await(5, TimeUnit.SECONDS))
        assertFalse(receiver.isOpen)

        a.close()
    }

    @Test
    fun `send on a closed link reports failure rather than throwing`() = runBlocking {
        val (a, b) = socketPair()
        val link = FramedSocketLink(a, scope(), onFrame = {})
        link.close()
        b.close()

        assertFalse(link.send("after-close".toByteArray()))
    }

    @Test
    fun `registry replaces a link for the same handle and closes the old one`() = runBlocking {
        val registry = SocketLinkRegistry()
        val (a1, b1) = socketPair()
        val (a2, b2) = socketPair()

        val first = FramedSocketLink(a1, scope(), onFrame = {})
        val second = FramedSocketLink(a2, scope(), onFrame = {})

        registry.put(1L, first)
        registry.put(1L, second)

        assertFalse(first.isOpen, "superseded link must not stay open")
        assertEquals(second, registry.get(1L))

        registry.close()
        assertFalse(second.isOpen)
        b1.close()
        b2.close()
    }

    @Test
    fun `registry hides a closed link from get`() = runBlocking {
        val registry = SocketLinkRegistry()
        val (a, b) = socketPair()
        val link = FramedSocketLink(a, scope(), onFrame = {})
        registry.put(7L, link)

        link.close()

        // A dead link must not be handed out — callers would silently drop
        // frames onto it instead of falling back to the outbox.
        assertEquals(null, registry.get(7L))

        registry.close()
        b.close()
    }
}
