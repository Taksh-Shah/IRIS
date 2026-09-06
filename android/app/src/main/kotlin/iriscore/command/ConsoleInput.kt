package iriscore.command

import androidx.compose.runtime.Immutable

/**
 * What the console input currently means.
 *
 * The mode is derived from the text itself rather than from a toggle the user
 * has to find: a leading `/` is a command, a leading `@` is a context
 * reference, anything else is a message. Typing is the mode switch, which is
 * what makes the CLI an interaction language instead of a separate screen.
 */
@Immutable
sealed interface InputMode {

    /** Ordinary human message. */
    data object Message : InputMode

    /**
     * Command entry. [token] is the partial command name after the slash and
     * before any whitespace; [argument] is everything after the first space.
     */
    data class Command(val token: String, val argument: String?) : InputMode

    /** Contextual reference entry — `@` followed by a partial peer reference. */
    data class Context(val token: String) : InputMode
}

/**
 * Parses raw input into a mode.
 *
 * Deliberately total and allocation-light: this runs on every keystroke.
 */
object ConsoleInputParser {

    const val COMMAND_PREFIX = '/'
    const val CONTEXT_PREFIX = '@'

    fun parse(raw: String): InputMode {
        // Only a *leading* prefix switches mode. A slash mid-sentence ("and/or")
        // and an email address must both stay ordinary text.
        return when {
            raw.startsWith(COMMAND_PREFIX) -> parseCommand(raw)
            raw.startsWith(CONTEXT_PREFIX) -> InputMode.Context(raw.drop(1).trim())
            else -> InputMode.Message
        }
    }

    private fun parseCommand(raw: String): InputMode.Command {
        val body = raw.drop(1)
        val split = body.indexOf(' ')
        return if (split < 0) {
            InputMode.Command(token = body, argument = null)
        } else {
            InputMode.Command(
                token = body.take(split),
                argument = body.drop(split + 1).takeIf { it.isNotBlank() }?.trim(),
            )
        }
    }
}

/**
 * Outcome of running a console line — what the console should do next.
 *
 * Returning a result rather than mutating state from the parser keeps command
 * execution testable without a Compose or Android runtime.
 */
@Immutable
sealed interface CommandResult {

    /** Send [text] at [priority]. */
    data class Send(val text: String, val priority: UByte) : CommandResult

    /** HV-41: send [text] at [priority] to every peer in range, not one recipient. */
    data class Broadcast(val text: String, val priority: UByte) : CommandResult

    /** Set the active recipient. */
    data class SetRecipient(val peerIdHex: String) : CommandResult

    /** HV-89 interim: register a peer's X25519 static key so we can seal mail to it. */
    data class AddKey(val peerIdHex: String, val x25519Hex: String) : CommandResult

    /** Filter the console view; null clears the filter. */
    data class Search(val query: String?) : CommandResult

    /** Emit a system event block into the console. */
    data class System(val title: String, val lines: List<Pair<String, String>>) : CommandResult

    /** Drain the relay outbox. */
    data object Relay : CommandResult

    /** Clear the console view. */
    data object Clear : CommandResult

    /** Report a problem to the user. */
    data class Error(val message: String) : CommandResult

    /** HV-56: save or update a contact name for a peer. */
    data class SaveContact(val peerIdHex: String, val name: String) : CommandResult

    /** HV-56: list all saved contacts. */
    data object ListContacts : CommandResult

    /** Recognised, but nothing to do (e.g. an empty line). */
    data object None : CommandResult
}
