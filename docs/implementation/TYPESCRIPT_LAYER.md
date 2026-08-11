# TypeScript / Tauri Desktop Layer

**Status:** Draft  
**Last updated:** 2026-08-11  
**Owner:** Desktop team  

---

## 1. Overview

IRIS desktop application uses [Tauri](https://tauri.app/) (v2.x): a framework that combines a Rust backend with a web-based frontend (HTML/CSS/JavaScript/TypeScript rendered in the OS webview). This gives IRIS a single deployment target for Windows, macOS, and Linux without shipping a bundled Chromium (unlike Electron).

The TypeScript layer handles **only UI**. All protocol logic (bundle parsing, routing, cryptography, transport) lives in the Rust `iris-core` crate, accessed via Tauri commands.

---

## 2. Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  IRIS Desktop App                                                │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  TypeScript / React (Renderer Process)                  │    │
│  │                                                         │    │
│  │  Components: SOSButton, Map, MessageList, StatusBar     │    │
│  │  State: Zustand store (local UI state)                  │    │
│  │  API: invoke() / listen() (Tauri IPC)                   │    │
│  └──────────────────────┬──────────────────────────────────┘    │
│                          │  Tauri IPC (JSON over postMessage)   │
│  ┌───────────────────────┴──────────────────────────────────┐   │
│  │  Rust / Tauri Main Process                               │   │
│  │                                                          │   │
│  │  Commands: #[tauri::command] handlers                    │   │
│  │  Events: app_handle.emit_all(...)                        │   │
│  │  iris-core: routing, crypto, bundle store                │   │
│  │  Transports: BLE (OS API), Wi-Fi Direct (OS API), LoRa  │   │
│  └──────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

---

## 3. Tauri IPC: Commands and Events

### 3.1 Commands (TypeScript → Rust)

Commands are Rust functions decorated with `#[tauri::command]` and registered in `main.rs`. TypeScript calls them via `invoke()`.

```typescript
// TypeScript (renderer)
import { invoke } from '@tauri-apps/api/core';

// Send SOS
const result = await invoke<SosResult>('send_sos', {
  note: 'Trapped on 3rd floor'
});

// Get current mesh status
const status = await invoke<MeshStatus>('get_mesh_status');

// Send a message
await invoke('send_message', {
  destinationNodeId: recipientId,
  plaintext: messageText,
  ttlSeconds: 604800
});
```

Defined commands in `src-tauri/src/commands/`:

| Command | Arguments | Return | Description |
|---------|----------|--------|-------------|
| `send_sos` | `note?: string` | `SosResult` | Send P0 SOS bundle |
| `get_mesh_status` | — | `MeshStatus` | Current relay count, peer count, transport status |
| `send_message` | `destination_node_id, plaintext, ttl_seconds` | `BundleId` | Send encrypted text message |
| `get_messages` | `page, page_size` | `Message[]` | Paginated message history |
| `get_contacts` | — | `Contact[]` | Known IRIS contacts (met via mesh) |
| `start_location_sharing` | `interval_seconds` | `()` | Begin periodic location broadcast |
| `stop_location_sharing` | — | `()` | Stop location sharing |
| `get_authority_broadcasts` | — | `AuthorityBroadcast[]` | Recent authority messages |
| `export_data` | `output_path: string` | `()` | Export own data to ZIP |
| `delete_all_data` | — | `()` | Wipe all local data |

### 3.2 Events (Rust → TypeScript)

Events are pushed from the Rust main process to the renderer via `app_handle.emit_all()`. TypeScript subscribes with `listen()`.

```typescript
import { listen } from '@tauri-apps/api/event';

// Listen for incoming SOS
const unlisten = await listen<SosEvent>('iris:sos-received', (event) => {
  showSOSAlert(event.payload);
});

// Listen for mesh status changes
await listen<MeshStatus>('iris:mesh-status-changed', (event) => {
  updateStatusBar(event.payload);
});
```

Defined events:

| Event | Payload | Trigger |
|-------|---------|---------|
| `iris:sos-received` | `SosEvent` | P0 bundle received from peer |
| `iris:message-received` | `Message` | Text message delivered to self |
| `iris:mesh-status-changed` | `MeshStatus` | Peer count or transport state changes |
| `iris:authority-broadcast` | `AuthorityBroadcast` | Verified authority broadcast received |
| `iris:relay-confirmed` | `RelayConfirmation` | A relay node confirmed forwarding own SOS |
| `iris:location-update` | `LocationUpdate` | Another node's location updated |
| `iris:bundle-delivered` | `BundleId` | Own bundle delivered (when ack received) |

---

## 4. Frontend Architecture

### 4.1 Technology Stack

| Layer | Library | Version | Purpose |
|-------|---------|---------|---------|
| Framework | React | 18.x | Component rendering |
| Language | TypeScript | 5.x | Type safety |
| State management | Zustand | 4.x | Global UI state (mesh status, contacts, messages) |
| Routing | React Router | 6.x | In-app navigation (map, messages, settings) |
| Styling | Tailwind CSS | 3.x | Utility-first CSS |
| Map | MapLibre GL JS | 3.x | Offline vector map rendering |
| Build | Vite | 5.x | Fast development server and bundler |

### 4.2 Component Structure

```
src/
├── components/
│   ├── emergency/
│   │   ├── SOSButton.tsx         # Large SOS button; always visible on main screen
│   │   ├── SOSConfirmation.tsx   # Full-screen confirmation after SOS send
│   │   └── AuthorityBroadcast.tsx # Full-screen P0 broadcast takeover
│   ├── map/
│   │   ├── EmergencyMap.tsx      # MapLibre GL; offline tiles; node locations
│   │   └── LocationMarker.tsx    # Individual node marker
│   ├── messaging/
│   │   ├── MessageList.tsx       # Paginated message history
│   │   ├── MessageComposer.tsx   # Text input + send
│   │   └── MessageBubble.tsx     # Individual message with delivery status
│   ├── status/
│   │   ├── MeshStatusBar.tsx     # Peer count, transport status, signal quality
│   │   └── BatteryIndicator.tsx  # Battery + low battery mode indicator
│   └── settings/
│       ├── PrivacySettings.tsx   # Consent management, data export, deletion
│       └── NetworkSettings.tsx   # Transport configuration, gateway address
├── stores/
│   ├── meshStore.ts              # Zustand: mesh status, peers, transports
│   ├── messageStore.ts           # Zustand: messages (local cache of SQLite data)
│   └── locationStore.ts         # Zustand: own and peers' locations
├── hooks/
│   ├── useTauriEvents.ts         # Subscribe to Tauri events on mount
│   └── useMeshStatus.ts          # Derived mesh status hook
└── App.tsx                       # Root component; router; event listener setup
```

### 4.3 State Management

Zustand stores are the source of truth for UI state. They are populated by:
1. Initial load: `invoke()` calls to fetch current state from Rust
2. Real-time updates: `listen()` event handlers that update store on push events from Rust

```typescript
// stores/meshStore.ts
interface MeshStore {
  status: MeshStatus;
  peers: Peer[];
  transports: TransportStatus[];
  setStatus: (status: MeshStatus) => void;
  setPeers: (peers: Peer[]) => void;
}

export const useMeshStore = create<MeshStore>((set) => ({
  status: { peerCount: 0, signalLevel: 'none', satelliteActive: false },
  peers: [],
  transports: [],
  setStatus: (status) => set({ status }),
  setPeers: (peers) => set({ peers }),
}));
```

---

## 5. Offline Map Integration

The desktop app uses MapLibre GL JS with locally-served vector tiles:

```typescript
// components/map/EmergencyMap.tsx
const map = new Map({
  container: mapContainerRef.current,
  style: 'http://localhost:8765/map-style.json',  // Tauri local asset server
  center: [78.96, 20.59],  // India center
  zoom: 4,
});
```

Map tiles are served by a local HTTP server in the Tauri main process (lightweight Axum server on port 8765, loopback only). This avoids the restriction on `file://` protocol for WebGL and MapLibre GL.

Tile data: India MBTiles file bundled with the installer (~200 MB for district-level coverage across India). Users can download state-level detail tiles (50–200 MB per state) via the app's Settings > Maps > Download.

---

## 6. Build System

### 6.1 Build Commands

```bash
# Development (hot reload)
cargo tauri dev

# Production build (current platform)
cargo tauri build

# Cross-platform build (CI)
cargo tauri build --target x86_64-pc-windows-msvc   # Windows
cargo tauri build --target aarch64-apple-darwin       # macOS ARM
cargo tauri build --target x86_64-unknown-linux-gnu   # Linux
```

### 6.2 CI Matrix

GitHub Actions workflow builds IRIS desktop on 3 platforms:

```yaml
strategy:
  matrix:
    include:
      - os: windows-latest
        target: x86_64-pc-windows-msvc
        artifact: iris-desktop_x64.msi
      - os: macos-latest
        target: aarch64-apple-darwin
        artifact: iris-desktop_arm64.dmg
      - os: ubuntu-22.04
        target: x86_64-unknown-linux-gnu
        artifact: iris-desktop_amd64.deb
```

### 6.3 Output Artifacts

| Platform | Installer format | Notes |
|----------|-----------------|-------|
| Windows | `.msi` (WiX) | Code-signed with EV certificate |
| macOS | `.dmg` (universal binary) | Notarized with Apple ID |
| Linux | `.deb` + `.AppImage` | `.deb` for Ubuntu/Debian; AppImage for universal |

---

## 7. Transport Access (TypeScript Does Not Touch Transports)

All transport operations (BLE, Wi-Fi Direct, LoRa) are handled entirely in the Rust main process. TypeScript has no access to OS networking APIs. This is an intentional architectural boundary:

- **Security:** Transport access is controlled by Rust; no JavaScript can exfiltrate data via transport
- **Simplicity:** Transport APIs (OS BLE APIs, socket APIs) are not available in the webview context
- **Cross-platform:** Rust handles OS differences; TypeScript sees a uniform command/event interface

TypeScript receives transport-related information only via events and commands from the Rust layer.

---

## 8. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
