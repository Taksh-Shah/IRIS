package iriscore.command

import iriscore.util.PeerIdCodec

/**
 * Turns a console line into a [CommandResult].
 *
 * Pure and free of Android and Compose types so the whole command surface is
 * unit-testable on the JVM — the previous shell had no way to test what a given
 * input would do short of running the app.
 */
object CommandExecutor {

    /** P0, the no-drop SOS tier (`ContentType::Sos`). */
    const val PRIORITY_SOS: UByte = 0u

    /** P4, matching `ContentType::Text.assign_priority()` in the core. */
    const val PRIORITY_NORMAL: UByte = 4u

    /** A PeerId is a 32-byte key rendered as hex. */
    const val PEER_ID_HEX_LENGTH = 64

    /**
     * @param raw the full input line.
     * @param hasRecipient whether a recipient is already selected — send
     *   commands fail cleanly rather than silently dropping the message.
     */
    fun execute(raw: String, hasRecipient: Boolean): CommandResult {
        val line = raw.trim()
        if (line.isEmpty()) return CommandResult.None

        return when (val mode = ConsoleInputParser.parse(line)) {
            is InputMode.Message -> sendOrReject(line, PRIORITY_NORMAL, hasRecipient)
            is InputMode.Context -> setRecipient(mode.token)
            is InputMode.Command -> runCommand(mode, hasRecipient)
        }
    }

    private fun runCommand(mode: InputMode.Command, hasRecipient: Boolean): CommandResult {
        val command = CommandRegistry.find(mode.token)
            ?: return CommandResult.Error("Unknown command /${mode.token} — try /help")

        return when (command.name) {
            "sos" -> {
                val text = mode.argument
                    ?: return CommandResult.Error("/sos needs a message")
                sendOrReject(text, PRIORITY_SOS, hasRecipient)
            }

            "send" -> {
                val text = mode.argument
                    ?: return CommandResult.Error("/send needs a message")
                sendOrReject(text, PRIORITY_NORMAL, hasRecipient)
            }

            "to" -> {
                val peer = mode.argument
                    ?: return CommandResult.Error("/to needs a PeerId")
                setRecipient(peer)
            }

            "search" -> CommandResult.Search(mode.argument)
            "clear" -> CommandResult.Clear
            "relay" -> CommandResult.Relay

            "help" -> CommandResult.System(
                title = "COMMANDS",
                lines = CommandRegistry.commands.map { it.invocation to it.description },
            )

            // `peers` and `node` are answered by the console, which holds the
            // live state; the executor only names what is being asked for.
            "peers" -> CommandResult.System(title = "LINKS", lines = emptyList())
            "node" -> CommandResult.System(title = "NODE", lines = emptyList())
            "diag" -> CommandResult.System(title = "DIAG", lines = emptyList())
            "stats" -> CommandResult.System(title = "STATS", lines = emptyList())

            else -> CommandResult.Error("/${command.name} is not wired up yet")
        }
    }

    private fun sendOrReject(text: String, priority: UByte, hasRecipient: Boolean): CommandResult =
        if (!hasRecipient) {
            CommandResult.Error("No recipient — set one with /to <peer-id> or @<peer-id>")
        } else {
            CommandResult.Send(text, priority)
        }

    private fun setRecipient(raw: String): CommandResult {
        val normalized = raw.trim().lowercase()
        return if (!PeerIdCodec.isHex(normalized) || normalized.length != PEER_ID_HEX_LENGTH) {
            CommandResult.Error("PeerId must be $PEER_ID_HEX_LENGTH hex characters")
        } else {
            CommandResult.SetRecipient(normalized)
        }
    }
}
