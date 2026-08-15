/* IRIS Mesh — vanilla frontend (DESKTOP_DESIGN.md §6). */
const { invoke, Channel } = window.__TAURI__.core;

const inbox = document.getElementById("inbox");
const empty = document.getElementById("empty");
const sendBtn = document.getElementById("send");
const statusNode = document.getElementById("node");
const statusOnline = document.getElementById("online");
const statusTransports = document.getElementById("transports");
const telemetryEl = document.getElementById("telemetry");

async function refreshStatus() {
  try {
    const s = await invoke("get_mesh_status");
    statusNode.textContent = "node: " + s.node_id_short;
    statusOnline.textContent = s.online ? "online" : "offline";
    statusOnline.className = s.online ? "on" : "off";
    statusTransports.textContent = s.transports
      .map((t) => t.display + " (" + t.state.toLowerCase() + ")")
      .join(" · ") || "no transports";
  } catch (e) {
    statusOnline.textContent = "error";
  }
}

async function refreshTelemetry() {
  try {
    const m = await invoke("get_telemetry");
    telemetryEl.textContent = m.length
      ? "telemetry: " + m.map((x) => x.name + "=" + x.value).join(" · ")
      : "telemetry: idle";
  } catch (e) {
    telemetryEl.textContent = "telemetry: unavailable";
  }
}

function appendInbox(view, mine) {
  if (empty) empty.remove();
  const el = document.createElement("div");
  el.className = "msg" + (mine ? " me" : "");
  const meta = document.createElement("div");
  meta.className = "meta";
  meta.textContent = `P${view.priority} from ${view.from_short} #${view.message_id_short}`;
  const body = document.createElement("div");
  body.className = "body";
  body.textContent = view.text ?? "[" + view.content_type + " message — payload not shown]";
  el.append(meta, body);
  inbox.append(el);
  inbox.scrollTop = inbox.scrollHeight;
}

async function send() {
  const recipient = document.getElementById("recipient").value.trim();
  const text = document.getElementById("text").value;
  const priority = Number(document.getElementById("priority").value);
  if (!recipient || !text) return;
  sendBtn.disabled = true;
  try {
    const id = await invoke("send_message", { recipient, text, priority });
    appendInbox(
      {
        priority,
        from_short: "me",
        message_id_short: id.id_hex,
        text,
        content_type: "Text",
      },
      true
    );
    document.getElementById("text").value = "";
  } catch (e) {
    appendInbox({ priority, from_short: "sys", message_id_short: "-", text: "send failed: " + e, content_type: "Text" }, true);
  } finally {
    sendBtn.disabled = false;
  }
}

async function subscribe() {
  const channel = new Channel();
  channel.onmessage = (view) => appendInbox(view, false);
  await invoke("subscribe_inbox", { channel });
}

sendBtn.addEventListener("click", send);
document.getElementById("text").addEventListener("keydown", (e) => {
  if (e.key === "Enter") send();
});

subscribe().catch(() => {});
refreshStatus();
refreshTelemetry();
setInterval(refreshStatus, 2000);
setInterval(refreshTelemetry, 2000);
