package iriscore.command

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Console input parsing, command ranking and execution.
 *
 * All pure JVM — the point of keeping the command surface free of Compose and
 * Android types is that "what does this input do" is answerable in a test
 * rather than only by running the app.
 */
class CommandEngineTest {

    private val peer = "a".repeat(64)

    // -- parsing -----------------------------------------------------------

    @Test
    fun `plain text is a message`() {
        assertEquals(InputMode.Message, ConsoleInputParser.parse("hello there"))
    }

    @Test
    fun `slash only switches mode at the start of the line`() {
        // "and/or" and a URL must stay ordinary prose, or the mode would flip
        // mid-sentence while someone is writing.
        assertEquals(InputMode.Message, ConsoleInputParser.parse("and/or"))
        assertEquals(InputMode.Message, ConsoleInputParser.parse("see http://x/y"))
    }

    @Test
    fun `at sign only switches mode at the start of the line`() {
        assertEquals(InputMode.Message, ConsoleInputParser.parse("mail me at a@b.com"))
    }

    @Test
    fun `bare command has no argument`() {
        assertEquals(InputMode.Command("help", null), ConsoleInputParser.parse("/help"))
    }

    @Test
    fun `command splits token from argument`() {
        assertEquals(
            InputMode.Command("search", "proposal draft"),
            ConsoleInputParser.parse("/search proposal draft"),
        )
    }

    @Test
    fun `trailing space does not fabricate an empty argument`() {
        assertEquals(InputMode.Command("relay", null), ConsoleInputParser.parse("/relay "))
    }

    @Test
    fun `context reference is parsed`() {
        assertEquals(InputMode.Context(peer), ConsoleInputParser.parse("@$peer"))
    }

    // -- registry search ---------------------------------------------------

    @Test
    fun `exact name outranks prefix and substring`() {
        assertEquals("send", CommandRegistry.search("send").first().name)
    }

    @Test
    fun `prefix matches rank above description matches`() {
        val names = CommandRegistry.search("se").map { it.name }
        // /search and /send both start with "se"; anything matched only via its
        // description must sort below them.
        assertTrue(names.take(2).containsAll(listOf("search", "send")), "got $names")
    }

    @Test
    fun `aliases resolve to their command`() {
        assertEquals("sos", CommandRegistry.find("p0")?.name)
        assertEquals("relay", CommandRegistry.find("sync")?.name)
        assertEquals("node", CommandRegistry.find("whoami")?.name)
    }

    @Test
    fun `empty query returns every command`() {
        assertEquals(CommandRegistry.commands.size, CommandRegistry.search("").size)
    }

    @Test
    fun `unknown query returns nothing`() {
        assertTrue(CommandRegistry.search("zzzznope").isEmpty())
    }

    // -- execution ---------------------------------------------------------

    @Test
    fun `message sends at normal priority not P0`() {
        val result = CommandExecutor.execute("hello", hasRecipient = true)
        assertEquals(
            CommandResult.Send("hello", CommandExecutor.PRIORITY_NORMAL),
            result,
        )
    }

    @Test
    fun `sos sends at P0`() {
        val result = CommandExecutor.execute("/sos trapped on floor 3", hasRecipient = true)
        assertEquals(
            CommandResult.Send("trapped on floor 3", CommandExecutor.PRIORITY_SOS),
            result,
        )
    }

    @Test
    fun `sos without a message is rejected rather than sending an empty P0`() {
        val result = CommandExecutor.execute("/sos", hasRecipient = true)
        assertTrue(result is CommandResult.Error)
    }

    @Test
    fun `sending without a recipient reports instead of dropping silently`() {
        val result = CommandExecutor.execute("hello", hasRecipient = false)
        assertTrue(result is CommandResult.Error)
    }

    @Test
    fun `recipient must be 64 hex`() {
        assertTrue(CommandExecutor.execute("/to abc", hasRecipient = false) is CommandResult.Error)
        assertTrue(CommandExecutor.execute("@zz", hasRecipient = false) is CommandResult.Error)
        assertEquals(
            CommandResult.SetRecipient(peer),
            CommandExecutor.execute("@$peer", hasRecipient = false),
        )
    }

    @Test
    fun `recipient is normalised to lower case`() {
        val result = CommandExecutor.execute("/to ${"A".repeat(64)}", hasRecipient = false)
        assertEquals(CommandResult.SetRecipient("a".repeat(64)), result)
    }

    @Test
    fun `unknown command is reported, not executed`() {
        val result = CommandExecutor.execute("/nonsense", hasRecipient = true)
        assertTrue(result is CommandResult.Error)
        assertTrue((result as CommandResult.Error).message.contains("/help"))
    }

    @Test
    fun `search with no argument clears the filter`() {
        assertEquals(CommandResult.Search(null), CommandExecutor.execute("/search", hasRecipient = true))
    }

    @Test
    fun `blank input does nothing`() {
        assertEquals(CommandResult.None, CommandExecutor.execute("   ", hasRecipient = true))
    }

    @Test
    fun `help lists every registered command`() {
        val result = CommandExecutor.execute("/help", hasRecipient = true)
        assertTrue(result is CommandResult.System)
        assertEquals(CommandRegistry.commands.size, (result as CommandResult.System).lines.size)
    }

    @Test
    fun `every registered command is executable`() {
        // Guards against a palette that advertises commands the executor does
        // not handle — the failure mode would be a command that silently does
        // nothing when selected.
        CommandRegistry.commands.forEach { command ->
            val line = if (command.argumentHint != null) {
                "${command.invocation} ${if (command.name == "to") peer else "x"}"
            } else {
                command.invocation
            }
            val result = CommandExecutor.execute(line, hasRecipient = true)
            assertNull(
                (result as? CommandResult.Error)?.message?.takeIf { it.contains("not wired up") },
                "${command.invocation} is advertised but not implemented",
            )
        }
    }
}
