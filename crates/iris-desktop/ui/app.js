/*
 * IRIS desktop shell.
 *
 * Keyboard-first: Ctrl/Cmd+K opens the palette, / and @ switch the composer
 * mode as you type, arrows navigate, Enter runs, Escape dismisses. Every
 * command is also reachable by pointer, so nothing is keyboard-only.
 *
 * Runs standalone in a plain browser (with the mesh disabled) so the interface
 * can be inspected without a Tauri host — `tauriInvoke` degrades instead of
 * throwing on `window.__TAURI__` being absent.
 */

const tauri = window.__TAURI__?.core ?? null;
const IS_MAC = navigator.platform.toUpperCase().includes("MAC");

const el = {
  transcript: document.getElementById("transcript"),
  empty: document.getElementById("empty"),
  input: document.getElementById("input"),
  composer: document.getElementById("composer"),
  sigil: document.getElementById("sigil"),
  composerHint: document.getElementById("composer-hint"),
  palette: document.getElementById("palette"),
  paletteList: document.getElementById("palette-list"),
  node: document.getElementById("node"),
  linkChip: document.getElementById("link-chip"),
  queueChip: document.getElementById("queue-chip"),
  recipientChip: document.getElementById("recipient-chip"),
  peerList: document.getElementById("peer-list"),
  peerCount: document.getElementById("peer-count"),
  ctxNode: document.getElementById("ctx-node"),
  ctxLink: document.getElementById("ctx-link"),
  ctxTransports: document.getElementById("ctx-transports"),
  ctxMetrics: document.getElementById("ctx-metrics"),
  toast: document.getElementById("toast"),
};

const state = {
  recipient: null,
  entries: [],
  peers: new Map(),
  matches: [],
  selected: 0,
  paletteOpen: false,
  searchQuery: null,
  status: null,
};

/* ---------------------------------------------------------------- plumbing */

async function tauriInvoke(command, args) {
  if (!tauri) throw new Error("mesh unavailable (running outside the app shell)");
  return tauri.invoke(command, args);
}

function toast(message) {
  el.toast.textContent = message;
  el.toast.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => {
    el.toast.hidden = true;
  }, 4000);
}

function timeOf(unixSeconds) {
  const d = unixSeconds ? new Date(unixSeconds * 1000) : new Date();
  return d.toTimeString().slice(0, 8);
}

/* --------------------------------------------------------------- rendering */

function addEntry(entry) {
  if (el.empty && el.empty.isConnected) el.empty.remove();
  state.entries.push(entry);
  // Bounded: a long session must not grow the transcript without limit.
  if (state.entries.length > 500) state.entries.shift();
  renderTranscript();
}

function matchesSearch(entry) {
  if (!state.searchQuery) return true;
  if (entry.kind !== "message") return true;
  return entry.text.toLowerCase().includes(state.searchQuery.toLowerCase());
}

function renderTranscript() {
  const atBottom =
    el.transcript.scrollHeight - el.transcript.scrollTop - el.transcript.clientHeight < 80;

  const visible = state.entries.filter(matchesSearch);
  el.transcript.replaceChildren(...visible.map((entry, i) => renderEntry(entry, visible[i - 1])));

  // Only follow the tail if the user was already there — yanking the viewport
  // away from someone reading scrollback is worse than a missed autoscroll.
  if (atBottom) el.transcript.scrollTop = el.transcript.scrollHeight;
}

function renderEntry(entry, previous) {
  const node = document.createElement("div");
  node.className = "entry";

  if (entry.kind === "message") {
    const grouped = previous && previous.kind === "message" && previous.from === entry.from;
    if (grouped) node.classList.add("grouped");

    if (!grouped) {
      const sender = document.createElement("div");
      sender.className = "msg-sender";
      sender.append(document.createTextNode(entry.from));
      if (entry.priority === IRIS_PRIORITY_SOS) sender.append(tag("P0", "sos"));
      if (entry.pending) sender.append(tag("QUEUED", "queued"));
      node.append(sender);
    }

    const body = document.createElement("div");
    body.className = "msg-body";
    body.textContent = entry.text;

    const time = document.createElement("div");
    time.className = "msg-time";
    time.textContent = timeOf(entry.at);

    node.append(body, time);
    return node;
  }

  if (entry.kind === "echo") {
    node.classList.add("echo");
    node.textContent = entry.text;
    return node;
  }

  // system
  if (entry.error) node.classList.add("error");
  const label = document.createElement("div");
  label.className = "sys-label";
  label.textContent = "IRIS";

  const title = document.createElement("div");
  title.className = "sys-title";
  title.textContent = entry.title;
  node.append(label, title);

  if (entry.lines?.length) {
    // Pad the key column so values align down the block, as they would in a
    // terminal.
    const width = Math.max(...entry.lines.map(([k]) => k.length));
    const pre = document.createElement("div");
    pre.className = "sys-lines";
    pre.textContent = entry.lines
      .map(([k, v]) => `> ${k.padEnd(width)}   ${v}`)
      .join("\n");
    node.append(pre);
  }

  if (entry.status) {
    const status = document.createElement("div");
    status.className = "sys-status";
    status.textContent = `STATUS: ${entry.status}`;
    node.append(status);
  }
  return node;
}

function tag(text, kind) {
  const span = document.createElement("span");
  span.className = `tag ${kind}`;
  span.textContent = text;
  return span;
}

function system(title, lines = [], status = null, error = false) {
  addEntry({ kind: "system", title, lines, status, error, at: Date.now() / 1000 });
}

/* ----------------------------------------------------------------- palette */

function renderPalette() {
  el.paletteList.replaceChildren();
  let lastGroup = null;

  state.matches.forEach((command, index) => {
    if (command.group !== lastGroup) {
      const group = document.createElement("div");
      group.className = "cmd-group";
      group.textContent = command.group;
      el.paletteList.append(group);
      lastGroup = command.group;
    }

    const row = document.createElement("button");
    row.type = "button";
    row.className = "cmd";
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(index === state.selected));

    const name = document.createElement("span");
    name.className = "cmd-name";
    name.textContent = `/${command.name}`;
    row.append(name);

    if (command.argumentHint) {
      const arg = document.createElement("span");
      arg.className = "cmd-arg";
      arg.textContent = command.argumentHint;
      row.append(arg);
    }

    const desc = document.createElement("span");
    desc.className = "cmd-desc";
    desc.textContent = command.description;
    row.append(desc);

    row.addEventListener("click", () => completeCommand(command));
    el.paletteList.append(row);

    if (index === state.selected) {
      requestAnimationFrame(() => row.scrollIntoView({ block: "nearest" }));
    }
  });
}

function setPaletteOpen(open) {
  state.paletteOpen = open;
  el.palette.hidden = !open;
}

function completeCommand(command) {
  // Completing leaves the cursor ready for the argument rather than running a
  // command that still needs one.
  el.input.value = command.argumentHint ? `/${command.name} ` : `/${command.name}`;
  el.input.focus();
  syncInput();
}

/* ------------------------------------------------------------------- input */

function syncInput() {
  const raw = el.input.value;
  const parsed = irisParseInput(raw);

  el.composer.classList.toggle("mode-command", parsed.kind === "command");
  el.composer.classList.toggle("mode-context", parsed.kind === "context");
  el.sigil.textContent = parsed.kind === "command" ? "/" : parsed.kind === "context" ? "@" : ">";

  if (parsed.kind === "command") {
    state.matches = irisSearchCommands(parsed.token);
    state.selected = Math.min(state.selected, Math.max(0, state.matches.length - 1));
    setPaletteOpen(state.matches.length > 0);
    renderPalette();
  } else {
    setPaletteOpen(false);
  }
}

async function submit() {
  const raw = el.input.value;
  if (!raw.trim()) return;

  if (raw.startsWith("/") || raw.startsWith("@")) {
    addEntry({ kind: "echo", text: raw.trim(), at: Date.now() / 1000 });
  }

  const result = irisExecute(raw, state.recipient !== null);
  el.input.value = "";
  state.selected = 0;
  syncInput();

  switch (result.type) {
    case "send":
      await sendMessage(result.text, result.priority);
      break;

    case "recipient":
      state.recipient = result.peerId;
      el.recipientChip.textContent = `→ ${result.peerId.slice(0, 8)}…`;
      el.recipientChip.hidden = false;
      system("RECIPIENT", [["peer", `${result.peerId.slice(0, 16)}…`]], "READY");
      break;

    case "search":
      state.searchQuery = result.query;
      system("SEARCH", [["query", result.query ?? "(cleared)"]]);
      renderTranscript();
      break;

    case "clear":
      state.entries = [];
      state.searchQuery = null;
      renderTranscript();
      break;

    case "relay":
      system("RELAY.DRAIN", [["note", "engine drains on its own cadence"]], "ACKNOWLEDGED");
      break;

    case "system":
      resolveSystem(result);
      break;

    case "error":
      system("ERROR", [["detail", result.message]], null, true);
      break;

    default:
      break;
  }
}

function resolveSystem(result) {
  const status = state.status;
  if (result.title === "NODE") {
    system(
      "NODE",
      [
        ["id", status?.node_id_short ?? "unknown"],
        ["online", String(Boolean(status?.online))],
      ],
      status?.online ? "UP" : "DOWN",
    );
    return;
  }
  if (result.title === "LINKS") {
    const transports = status?.transports ?? [];
    system(
      "LINKS",
      transports.length
        ? transports.map((t) => [t.display, t.state.toLowerCase()])
        : [["transports", "none"]],
      status?.online ? "UP" : "DOWN",
    );
    return;
  }
  system(result.title, result.lines);
}

async function sendMessage(text, priority) {
  try {
    const id = await tauriInvoke("send_message", {
      recipient: state.recipient,
      text,
      priority,
    });
    addEntry({
      kind: "message",
      from: "me",
      text,
      priority,
      at: Date.now() / 1000,
      id: id?.id_hex,
    });
  } catch (e) {
    system("SEND FAILED", [["detail", String(e)]], null, true);
  }
}

/* --------------------------------------------------------------- keyboard */

document.addEventListener("keydown", (event) => {
  const mod = IS_MAC ? event.metaKey : event.ctrlKey;

  // Ctrl/Cmd+K focuses the composer in command mode from anywhere.
  if (mod && event.key.toLowerCase() === "k") {
    event.preventDefault();
    el.input.focus();
    if (!el.input.value.startsWith("/")) el.input.value = "/";
    syncInput();
    return;
  }

  if (event.key === "Escape") {
    if (state.paletteOpen) {
      setPaletteOpen(false);
      event.preventDefault();
    } else if (state.searchQuery) {
      state.searchQuery = null;
      renderTranscript();
    }
    return;
  }

  if (document.activeElement !== el.input) return;

  if (state.paletteOpen && (event.key === "ArrowDown" || event.key === "ArrowUp")) {
    event.preventDefault();
    const delta = event.key === "ArrowDown" ? 1 : -1;
    const count = state.matches.length;
    if (count > 0) {
      state.selected = (state.selected + delta + count) % count;
      renderPalette();
    }
    return;
  }

  if (event.key === "Enter") {
    event.preventDefault();
    // Enter completes the highlighted command rather than running a partial
    // one; a second Enter runs it.
    if (state.paletteOpen && state.matches.length > 0) {
      const command = state.matches[state.selected];
      const parsed = irisParseInput(el.input.value);
      if (parsed.kind === "command" && parsed.token !== command.name && !parsed.argument) {
        completeCommand(command);
        return;
      }
    }
    submit();
  }
});

el.input.addEventListener("input", syncInput);

/* ------------------------------------------------------------------ status */

function renderStatus(status) {
  state.status = status;
  el.node.textContent = status.node_id_short ?? "…";
  el.ctxNode.textContent = status.node_id_short ?? "…";

  el.linkChip.textContent = status.online ? "LINK" : "DOWN";
  el.linkChip.className = `chip ${status.online ? "on" : "off"}`;
  el.ctxLink.textContent = status.online ? "up" : "down";

  el.ctxTransports.replaceChildren(
    ...(status.transports?.length
      ? status.transports.map((t) => ctxRow(t.display, t.state.toLowerCase()))
      : [emptyNote("none")]),
  );

  const m = status.metrics;
  el.ctxMetrics.replaceChildren(
    ...(m
      ? [
          ctxRow("sent", m.sent),
          ctxRow("delivered", m.delivered),
          ctxRow("relayed", m.relayed),
          ctxRow("duplicates", m.dropped_duplicates),
          ctxRow("expired", m.expired),
          ctxRow("failed", m.delivery_failed),
        ]
      : [emptyNote("idle")]),
  );
}

function ctxRow(key, value) {
  const row = document.createElement("div");
  row.className = "ctx-row";
  const k = document.createElement("span");
  k.className = "ctx-key";
  k.textContent = key;
  const v = document.createElement("span");
  v.className = "ctx-val";
  v.textContent = String(value);
  row.append(k, v);
  return row;
}

function emptyNote(text) {
  const p = document.createElement("p");
  p.className = "pane-empty";
  p.textContent = text;
  return p;
}

function notePeer(shortId) {
  if (!shortId || state.peers.has(shortId)) return;
  state.peers.set(shortId, { seen: Date.now() });
  renderPeers();
}

function renderPeers() {
  el.peerCount.textContent = String(state.peers.size);
  if (state.peers.size === 0) return;

  el.peerList.replaceChildren(
    ...[...state.peers.entries()].map(([id, meta]) => {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "peer";
      row.setAttribute("role", "option");
      row.setAttribute("aria-selected", String(state.recipient?.startsWith(id) ?? false));

      const name = document.createElement("span");
      name.className = "peer-id";
      name.textContent = id;

      const sub = document.createElement("span");
      sub.className = "peer-meta";
      sub.textContent = `seen ${timeOf(meta.seen / 1000)}`;

      row.append(name, sub);
      // A short id is a display truncation, not an addressable PeerId — the
      // full 64-hex id has to come from /to, so selecting only prefills.
      row.addEventListener("click", () => {
        el.input.value = `/to ${id}`;
        el.input.focus();
        syncInput();
      });
      return row;
    }),
  );
}

/* -------------------------------------------------------------- lifecycle */

async function refreshStatus() {
  try {
    renderStatus(await tauriInvoke("get_mesh_status"));
  } catch {
    el.linkChip.textContent = "DOWN";
    el.linkChip.className = "chip off";
  }
}

async function subscribeInbox() {
  const Channel = window.__TAURI__?.core?.Channel;
  if (!Channel) return;
  const channel = new Channel();
  channel.onmessage = (view) => {
    notePeer(view.from_short);
    addEntry({
      kind: "message",
      from: view.from_short,
      text: view.text ?? `[${view.content_type} payload not shown]`,
      priority: view.priority,
      at: view.received_at_unix,
      id: view.message_id_short,
    });
  };
  await tauriInvoke("subscribe_inbox", { channel });
}

el.composerHint.textContent = IS_MAC ? "⌘K" : "Ctrl K";
syncInput();
el.input.focus();

if (tauri) {
  subscribeInbox().catch(() => {});
  refreshStatus();
  setInterval(refreshStatus, 2000);
} else {
  system(
    "OFFLINE",
    [["shell", "no mesh host attached"]],
    "UI ONLY",
  );
}
