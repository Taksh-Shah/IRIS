package iriscore.ui.state

import androidx.compose.runtime.Immutable
import java.util.concurrent.atomic.AtomicLong

/**
 * One line in the console.
 *
 * Messages and system output share a single ordered stream rather than living
 * in separate panes: running `/help` or `/peers` should read as part of the
 * conversation, not as a modal that interrupts it. That is what keeps commands,
 * system state and human communication feeling like one surface.
 */
@Immutable
sealed interface ConsoleEntry {

    /** Stable identity for `LazyColumn` keys. */
    val uid: Long

    /** Ordering key. */
    val atMs: Long

    @Immutable
    data class Message(val message: InboxUiMessage) : ConsoleEntry {
        override val uid: Long get() = message.uid
        override val atMs: Long get() = message.receivedAtMs
    }

    @Immutable
    data class System(
        override val uid: Long,
        override val atMs: Long,
        val title: String,
        val lines: List<Pair<String, String>>,
        val status: String? = null,
    ) : ConsoleEntry

    /** The user's own command line, echoed so the transcript reads back. */
    @Immutable
    data class Echo(
        override val uid: Long,
        override val atMs: Long,
        val text: String,
    ) : ConsoleEntry

    companion object {
        private val uids = AtomicLong(1L shl 40) // disjoint from InboxUiMessage ids

        fun nextUid(): Long = uids.getAndIncrement()

        // Fully qualified: the nested `System` entry type shadows java.lang.System
        // inside this scope.
        fun system(
            title: String,
            lines: List<Pair<String, String>> = emptyList(),
            status: String? = null,
            atMs: Long = java.lang.System.currentTimeMillis(),
        ): System = System(nextUid(), atMs, title, lines, status)

        fun echo(text: String, atMs: Long = java.lang.System.currentTimeMillis()): Echo =
            Echo(nextUid(), atMs, text)
    }
}
