/*
 * IRIS command engine — desktop.
 *
 * Mirrors android/app/src/main/kotlin/iriscore/command/ (CommandRegistry,
 * ConsoleInputParser, CommandExecutor). The two platforms must expose the same
 * command surface and the same parsing rules, so a user who learns /sos on the
 * phone finds it behaving identically here. `DesignTokenParityTest` on the
 * Android side asserts the two command sets stay in step.
 *
 * Commands are data. Adding one is a registry entry — the palette, ranking,
 * keyboard navigation and pointer selection all follow from it.
 */

const IRIS_PRIORITY_SOS = 0; // ContentType::Sos
const IRIS_PRIORITY_NORMAL = 4; // ContentType::Text.assign_priority()
const IRIS_PEER_ID_HEX_LENGTH = 64;

const IRIS_COMMANDS = [
  {
    name: "sos",
    description: "Send at P0 — no-drop emergency tier",
    group: "MESSAGE",
    aliases: ["emergency", "p0", "alert"],
    argumentHint: "<message>",
  },
  {
    name: "send",
    description: "Send at normal priority",
    group: "MESSAGE",
    aliases: [],
    argumentHint: "<message>",
  },
  {
    name: "to",
    description: "Set the recipient PeerId",
    group: "MESSAGE",
    aliases: ["recipient", "peer"],
    argumentHint: "<peer-id>",
  },
  {
    name: "search",
    description: "Search messages in this thread",
    group: "THREAD",
    aliases: ["find", "grep"],
    argumentHint: "<query>",
  },
  { name: "clear", description: "Clear the console view", group: "THREAD", aliases: [] },
  {
    name: "relay",
    description: "Drain the relay outbox now",
    group: "NETWORK",
    aliases: ["sync", "flush"],
  },
  {
    name: "peers",
    description: "Show transport and link state",
    group: "NETWORK",
    aliases: ["links", "status"],
  },
  {
    name: "diag",
    description: "Full mesh diagnostic — transports, neighbours, counters",
    group: "NETWORK",
    aliases: ["diagnostic", "snapshot"],
  },
  {
    name: "stats",
    description: "Message / route / security counters (non-zero)",
    group: "NETWORK",
    aliases: ["kpi", "metrics"],
  },
  {
    name: "node",
    description: "Show this node's identity",
    group: "SYSTEM",
    aliases: ["id", "whoami"],
  },
  { name: "help", description: "List available commands", group: "SYSTEM", aliases: ["?"] },
];

const IRIS_COMMANDS_BY_NAME = (() => {
  const map = new Map();
  for (const command of IRIS_COMMANDS) {
    map.set(command.name, command);
    for (const alias of command.aliases) map.set(alias, command);
  }
  return map;
})();

function irisFindCommand(name) {
  return IRIS_COMMANDS_BY_NAME.get(String(name).toLowerCase()) ?? null;
}

/**
 * Ranked matches for a partial command. Prefix-first, then alias, then
 * substring: typing "se" must surface /send and /search above anything that
 * merely mentions "se" in its description.
 */
function irisSearchCommands(query) {
  const q = String(query).trim().replace(/^\//, "").toLowerCase();
  if (!q) return IRIS_COMMANDS.slice();

  const rankOf = (command) => {
    if (command.name === q) return 0;
    if (command.name.startsWith(q)) return 1;
    if (command.aliases.includes(q)) return 2;
    if (command.aliases.some((a) => a.startsWith(q))) return 3;
    if (command.name.includes(q)) return 4;
    if (command.description.toLowerCase().includes(q)) return 5;
    return null;
  };

  return IRIS_COMMANDS.map((command) => ({ command, rank: rankOf(command) }))
    .filter((entry) => entry.rank !== null)
    .sort((a, b) => a.rank - b.rank || a.command.name.localeCompare(b.command.name))
    .map((entry) => entry.command);
}

/**
 * Derives what the input currently means. Only a *leading* sigil switches mode,
 * so "and/or" and an email address stay ordinary prose.
 */
function irisParseInput(raw) {
  if (raw.startsWith("/")) {
    const body = raw.slice(1);
    const split = body.indexOf(" ");
    if (split < 0) return { kind: "command", token: body, argument: null };
    const argument = body.slice(split + 1).trim();
    return {
      kind: "command",
      token: body.slice(0, split),
      argument: argument.length ? argument : null,
    };
  }
  if (raw.startsWith("@")) return { kind: "context", token: raw.slice(1).trim() };
  return { kind: "message" };
}

function irisIsPeerId(value) {
  return (
    typeof value === "string" &&
    value.length === IRIS_PEER_ID_HEX_LENGTH &&
    /^[0-9a-f]+$/i.test(value)
  );
}

/**
 * Executes a console line, returning an effect for the shell to apply.
 * Pure — no DOM, no IPC — so it is testable in isolation.
 */
function irisExecute(raw, hasRecipient) {
  const line = String(raw).trim();
  if (!line) return { type: "none" };

  const sendOrReject = (text, priority) =>
    hasRecipient
      ? { type: "send", text, priority }
      : { type: "error", message: "No recipient — set one with /to <peer-id> or @<peer-id>" };

  const setRecipient = (value) => {
    const normalized = String(value).trim().toLowerCase();
    return irisIsPeerId(normalized)
      ? { type: "recipient", peerId: normalized }
      : { type: "error", message: `PeerId must be ${IRIS_PEER_ID_HEX_LENGTH} hex characters` };
  };

  const parsed = irisParseInput(line);
  if (parsed.kind === "message") return sendOrReject(line, IRIS_PRIORITY_NORMAL);
  if (parsed.kind === "context") return setRecipient(parsed.token);

  const command = irisFindCommand(parsed.token);
  if (!command) {
    return { type: "error", message: `Unknown command /${parsed.token} — try /help` };
  }

  switch (command.name) {
    case "sos":
      return parsed.argument
        ? sendOrReject(parsed.argument, IRIS_PRIORITY_SOS)
        : { type: "error", message: "/sos needs a message" };
    case "send":
      return parsed.argument
        ? sendOrReject(parsed.argument, IRIS_PRIORITY_NORMAL)
        : { type: "error", message: "/send needs a message" };
    case "to":
      return parsed.argument
        ? setRecipient(parsed.argument)
        : { type: "error", message: "/to needs a PeerId" };
    case "search":
      return { type: "search", query: parsed.argument };
    case "clear":
      return { type: "clear" };
    case "relay":
      return { type: "relay" };
    case "help":
      return {
        type: "system",
        title: "COMMANDS",
        lines: IRIS_COMMANDS.map((c) => [`/${c.name}`, c.description]),
      };
    case "peers":
      return { type: "system", title: "LINKS", lines: [] };
    case "diag":
      return { type: "system", title: "DIAG", lines: [] };
    case "stats":
      return { type: "system", title: "STATS", lines: [] };
    case "node":
      return { type: "system", title: "NODE", lines: [] };
    default:
      return { type: "error", message: `/${command.name} is not wired up yet` };
  }
}

// Exported for the Node-based parity test; harmless in the browser.
if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    IRIS_COMMANDS,
    IRIS_PRIORITY_SOS,
    IRIS_PRIORITY_NORMAL,
    irisFindCommand,
    irisSearchCommands,
    irisParseInput,
    irisExecute,
  };
}
