package iriscore.data

import com.google.common.truth.Truth.assertThat
import iriscore.data.db.PendingMessageDao
import iriscore.data.db.PendingMessageEntity
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test

/**
 * WP13: [RelayOutbox]'s bounded-retry give-up behaviour, exercised against a
 * fake in-memory [PendingMessageDao] so this runs as a plain JVM test with
 * no Room/Android framework — covers the exact regression WP11 fixed:
 * "queued rows never transition after the queue drains" (a permanently
 * failing send used to either loop forever or silently stop the whole
 * drain for every other queued message behind it).
 */
class RelayOutboxTest {

    /** Minimal fake — enough of [PendingMessageDao]'s contract for [RelayOutbox]. */
    private class FakeDao : PendingMessageDao {
        private val rows = mutableListOf<PendingMessageEntity>()
        private var nextId = 1L
        val countFlow = MutableStateFlow(0)

        override suspend fun insert(message: PendingMessageEntity): Long {
            val withId = message.copy(id = nextId++)
            rows.add(withId)
            countFlow.value = rows.size
            return withId.id
        }

        override suspend fun getAll(): List<PendingMessageEntity> = rows.toList()

        override suspend fun delete(message: PendingMessageEntity) {
            rows.removeAll { it.id == message.id }
            countFlow.value = rows.size
        }

        override suspend fun incrementAttempts(id: Long) {
            val idx = rows.indexOfFirst { it.id == id }
            if (idx >= 0) rows[idx] = rows[idx].copy(attempts = rows[idx].attempts + 1)
        }

        override suspend fun removeRecipient(recipientHex: String) {
            rows.removeAll { it.recipientHex.equals(recipientHex, ignoreCase = true) }
            countFlow.value = rows.size
        }

        override suspend fun removeByMessageUid(messageUid: Long) {
            rows.removeAll { it.messageUid == messageUid }
            countFlow.value = rows.size
        }

        override fun observeCount(): Flow<Int> = countFlow
    }

    private fun queued(uid: Long, peer: String = "a".repeat(64)) =
        RelayOutbox.QueuedMessage(uid, peer, "hello", 4u)

    @Test
    fun `a successful send is removed from the outbox`() = runTest {
        val outbox = RelayOutbox(FakeDao())
        outbox.enqueue(queued(uid = 1))

        val sent = outbox.drain(onSend = { true }, onGiveUp = { })

        assertThat(sent).isEqualTo(1)
        assertThat(outbox.size).isEqualTo(0)
    }

    @Test
    fun `a failing row is retried, not deleted, until it exceeds MAX_DRAIN_ATTEMPTS`() = runTest {
        val outbox = RelayOutbox(FakeDao())
        outbox.enqueue(queued(uid = 1))

        var gaveUpOn: Long? = null
        // One pass short of the give-up threshold: still present, not given up on.
        repeat(RelayOutbox.MAX_DRAIN_ATTEMPTS - 1) {
            outbox.drain(onSend = { false }, onGiveUp = { gaveUpOn = it })
        }
        assertThat(outbox.size).isEqualTo(1)
        assertThat(gaveUpOn).isNull()

        // The pass that crosses the threshold gives up and removes the row.
        outbox.drain(onSend = { false }, onGiveUp = { gaveUpOn = it })
        assertThat(gaveUpOn).isEqualTo(1L)
        assertThat(outbox.size).isEqualTo(0)
    }

    @Test
    fun `one permanently-failing row does not block other queued sends from draining`() = runTest {
        // Regression for the audit's "queued rows never transition after the
        // queue drains": the old drain() stopped the ENTIRE loop on the first
        // failure, so a bad recipient at the front of the FIFO queue could
        // starve every legitimate send behind it.
        val outbox = RelayOutbox(FakeDao())
        outbox.enqueue(queued(uid = 1, peer = "bad".padEnd(64, '0')))
        outbox.enqueue(queued(uid = 2, peer = "good".padEnd(64, '0')))

        val sent = outbox.drain(
            onSend = { it.messageUid != 1L },
            onGiveUp = { },
        )

        assertThat(sent).isEqualTo(1)
        // uid 1 stays queued (not yet at the give-up threshold); uid 2 is gone.
        assertThat(outbox.size).isEqualTo(1)
    }
}
