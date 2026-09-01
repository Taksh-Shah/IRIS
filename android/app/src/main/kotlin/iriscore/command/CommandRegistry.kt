package iriscore.command

import androidx.compose.runtime.Immutable

/**
 * A command the console can execute.
 *
 * Commands are data, not bespoke UI. Adding one means adding an entry to
 * [CommandRegistry]; the palette, filtering, keyboard navigation and the touch
 * equivalent all follow automatically. Nothing renders per-command chrome.
 */
@Immutable
data class IrisCommand(
    /** Invocation name without the leading slash, e.g. `search`. */
    val name: String,
    val description: String,
    val group: CommandGroup,
    /** Alternate spellings that should match in search, e.g. `sos` -> `alert`. */
    val aliases: List<String> = emptyList(),
    /** Argument hint shown inline, e.g. `<query>`. Null for bare commands. */
    val argumentHint: String? = null,
    /**
     * False when the command cannot run in the current context — it stays
     * visible but unselectable, so the surface does not appear to lose
     * capabilities as state changes.
     */
    val enabled: Boolean = true,
) {
    /** Canonical typed form. */
    val invocation: String get() = "/$name"
}

@Immutable
enum class CommandGroup(val label: String) {
    MESSAGE("MESSAGE"),
    THREAD("THREAD"),
    NETWORK("NETWORK"),
    SYSTEM("SYSTEM"),
}

/**
 * The command set.
 *
 * Kept deliberately small. Every entry here has a real handler in the console;
 * a palette listing commands that do nothing is worse than a shorter palette.
 */
object CommandRegistry {

    val commands: List<IrisCommand> = listOf(
        IrisCommand(
            name = "sos",
            description = "Send at P0 — no-drop emergency tier",
            group = CommandGroup.MESSAGE,
            aliases = listOf("emergency", "p0", "alert"),
            argumentHint = "<message>",
        ),
        IrisCommand(
            name = "send",
            description = "Send at normal priority",
            group = CommandGroup.MESSAGE,
            argumentHint = "<message>",
        ),
        IrisCommand(
            name = "to",
            description = "Set the recipient PeerId",
            group = CommandGroup.MESSAGE,
            aliases = listOf("recipient", "peer"),
            argumentHint = "<peer-id>",
        ),
        IrisCommand(
            name = "x25519",
            description = "Show this node's X25519 key (hand to a peer for /addkey)",
            group = CommandGroup.SYSTEM,
            aliases = listOf("mykey", "key"),
        ),
        IrisCommand(
            name = "addkey",
            description = "Trust a peer's X25519 key so you can send to it",
            group = CommandGroup.MESSAGE,
            aliases = listOf("trust"),
            argumentHint = "<peer-id> <x25519>",
        ),
        IrisCommand(
            name = "search",
            description = "Search messages in this thread",
            group = CommandGroup.THREAD,
            aliases = listOf("find", "grep"),
            argumentHint = "<query>",
        ),
        IrisCommand(
            name = "clear",
            description = "Clear the console view",
            group = CommandGroup.THREAD,
        ),
        IrisCommand(
            name = "relay",
            description = "Drain the relay outbox now",
            group = CommandGroup.NETWORK,
            aliases = listOf("sync", "flush"),
        ),
        IrisCommand(
            name = "peers",
            description = "Show transport and link state",
            group = CommandGroup.NETWORK,
            aliases = listOf("links", "status"),
        ),
        IrisCommand(
            name = "diag",
            description = "Full mesh diagnostic — transports, neighbours, counters",
            group = CommandGroup.NETWORK,
            aliases = listOf("diagnostic", "snapshot"),
        ),
        IrisCommand(
            name = "stats",
            description = "Message / route / security counters (non-zero)",
            group = CommandGroup.NETWORK,
            aliases = listOf("kpi", "metrics"),
        ),
        IrisCommand(
            name = "node",
            description = "Show this node's identity",
            group = CommandGroup.SYSTEM,
            aliases = listOf("id", "whoami"),
        ),
        IrisCommand(
            name = "help",
            description = "List available commands",
            group = CommandGroup.SYSTEM,
            aliases = listOf("?"),
        ),
    )

    private val byName: Map<String, IrisCommand> = buildMap {
        commands.forEach { command ->
            put(command.name, command)
            command.aliases.forEach { put(it, command) }
        }
    }

    fun find(name: String): IrisCommand? = byName[name.lowercase()]

    /**
     * Ranked matches for a partial command.
     *
     * Ranking is prefix-first, then substring, then alias: typing `se` should
     * put `/send` and `/search` above a command that merely contains "se"
     * somewhere in its description.
     */
    fun search(query: String): List<IrisCommand> {
        val q = query.trim().removePrefix("/").lowercase()
        if (q.isEmpty()) return commands

        return commands
            .mapNotNull { command ->
                val rank = rankOf(command, q) ?: return@mapNotNull null
                command to rank
            }
            .sortedWith(compareBy({ it.second }, { it.first.name }))
            .map { it.first }
    }

    private fun rankOf(command: IrisCommand, q: String): Int? = when {
        command.name == q -> 0
        command.name.startsWith(q) -> 1
        command.aliases.any { it == q } -> 2
        command.aliases.any { it.startsWith(q) } -> 3
        command.name.contains(q) -> 4
        command.description.lowercase().contains(q) -> 5
        else -> null
    }
}
