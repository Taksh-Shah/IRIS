# IRIS Windows Platform Documentation

## Platform Targets

- **Minimum Windows**: Windows 10 version 1903 (build 18362)
- **Architecture**: x86_64 (primary), ARM64 (future)
- **Framework**: Tauri 2.0 (Rust backend + WebView2 frontend)
- **Frontend**: TypeScript + React 18
- **Rust toolchain**: stable-x86_64-pc-windows-msvc

## Architecture

### Stack

```
UI:             React 18 + TypeScript (rendered in WebView2)
IPC:            Tauri invoke() — TypeScript → Rust command handlers
Rust backend:   Tauri 2.0 app + iris-core (workspace crate)
BLE:            WinRT Windows.Devices.Bluetooth namespace
Wi-Fi Direct:   Windows.Devices.WiFiDirect namespace
Networking:     Windows.Networking.Sockets / WinSock2
Key storage:    Windows Credential Manager / DPAPI
Storage:        SQLite via rusqlite in Rust core
Background:     Windows system tray (persistent process)
```

### Directory Structure

```
desktop/
├── src-tauri/
│   ├── Cargo.toml               # Tauri + iris-core dependency
│   ├── tauri.conf.json          # Tauri configuration
│   ├── build.rs                 # Build script
│   └── src/
│       ├── main.rs              # Tauri app entry point
│       ├── commands/            # Tauri command handlers
│       │   ├── messaging.rs     # send_message, get_messages
│       │   ├── identity.rs      # get_node_id, rotate_keys
│       │   └── network.rs       # get_neighbors, get_status
│       ├── transport/
│       │   ├── ble_windows.rs   # WinRT BLE implementation
│       │   └── wifi_direct.rs   # Wi-Fi Direct implementation
│       └── tray.rs              # System tray icon and menu
└── src/
    ├── App.tsx
    ├── components/
    └── hooks/
        └── useIris.ts           # Tauri invoke wrappers
```

## Tauri 2.0 Architecture

Tauri wraps a Rust binary (the backend) and a WebView2 webview (the frontend). Communication is via Tauri's typed command system.

### Command Definition (Rust)

```rust
use tauri::State;
use iris_core::{IrisCoreHandle, MessagePriority};

#[tauri::command]
async fn send_message(
    state: State<'_, IrisCoreHandle>,
    recipient_id: String,
    payload: Vec<u8>,
    priority: u8,
) -> Result<String, String> {
    let priority = MessagePriority::try_from(priority)
        .map_err(|e| e.to_string())?;
    state.send_message(&recipient_id, &payload, priority)
        .await
        .map(|id| id.to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_neighbors(state: State<'_, IrisCoreHandle>) -> Vec<NeighborInfo> {
    state.get_neighbors()
}
```

### Command Invocation (TypeScript)

```typescript
import { invoke } from '@tauri-apps/api/core';

interface NeighborInfo {
  nodeId: string;
  transport: string;
  lastSeenMs: number;
  rssi?: number;
}

async function sendMessage(
  recipientId: string,
  payload: Uint8Array,
  priority: number
): Promise<string> {
  return invoke<string>('send_message', {
    recipientId,
    payload: Array.from(payload),
    priority,
  });
}

async function getNeighbors(): Promise<NeighborInfo[]> {
  return invoke<NeighborInfo[]>('get_neighbors');
}
```

### Tauri Events (Rust → TypeScript push)

```rust
// In Rust, emit event when message received
app_handle.emit("message_received", MessageReceivedPayload {
    message_id: msg.id.to_string(),
    sender_id: msg.sender.to_hex(),
    priority: msg.priority as u8,
    preview: msg.preview_text(),
}).unwrap();
```

```typescript
// In TypeScript, listen for events
import { listen } from '@tauri-apps/api/event';

await listen<MessageReceivedPayload>('message_received', (event) => {
  dispatch(addMessage(event.payload));
  showNotification(event.payload);
});
```

## BLE on Windows

### WinRT Bluetooth APIs

Windows BLE is accessed via WinRT (Windows Runtime) APIs. In Rust, use the `windows` crate:

```toml
[dependencies]
windows = { version = "0.52", features = [
    "Devices_Bluetooth",
    "Devices_Bluetooth_Advertisement",
    "Devices_Bluetooth_GenericAttributeProfile",
    "Devices_Enumeration",
    "Foundation_Collections",
    "Storage_Streams",
]}
```

### BLE Scanning (Observer)

```rust
use windows::Devices::Bluetooth::Advertisement::*;

let watcher = BluetoothLEAdvertisementWatcher::new()?;
let filter = BluetoothLEAdvertisementFilter::new()?;
let advertisement = BluetoothLEAdvertisement::new()?;
advertisement.ServiceUuids()?.Append(IRIS_SERVICE_GUID)?;
filter.SetAdvertisement(&advertisement)?;
watcher.SetAdvertisementFilter(&filter)?;
watcher.SetScanningMode(BluetoothLEScanningMode::Active)?;

watcher.Received(&TypedEventHandler::new(move |_, args: &Option<BluetoothLEAdvertisementReceivedEventArgs>| {
    if let Some(args) = args {
        let addr = args.BluetoothAddress()?;
        let rssi = args.RawSignalStrengthInDBBm()?;
        // Process advertisement
    }
    Ok(())
}))?;

watcher.Start()?;
```

### BLE Advertising (Publisher)

Windows BLE advertising is limited compared to Android/iOS. The `BluetoothLEAdvertisementPublisher` can advertise but:
- No GATT server via publisher API directly
- For GATT server: use `GattServiceProvider`

```rust
use windows::Devices::Bluetooth::GenericAttributeProfile::*;

let params = GattServiceProviderAdvertisingParameters::new()?;
params.SetIsConnectable(true)?;
params.SetIsDiscoverable(true)?;

let result = GattServiceProvider::CreateAsync(IRIS_SERVICE_GUID)?.await?;
let service_provider = result.ServiceProvider()?;
service_provider.StartAdvertising(&params)?;
```

### GATT Client

```rust
let device = BluetoothLEDevice::FromBluetoothAddressAsync(bluetooth_address)?.await?;
let services_result = device.GetGattServicesForUuidAsync(IRIS_SERVICE_GUID)?.await?;
let service = services_result.Services()?.GetAt(0)?;
let chars_result = service.GetCharacteristicsForUuidAsync(IRIS_MESSAGE_WRITE_GUID)?.await?;
let write_char = chars_result.Characteristics()?.GetAt(0)?;

// Write data
let writer = DataWriter::new()?;
writer.WriteBytes(data)?;
let buffer = writer.DetachBuffer()?;
write_char.WriteValueWithOptionAsync(&buffer, GattWriteOption::WriteWithResponse)?.await?;
```

## Wi-Fi Direct on Windows

```rust
use windows::Devices::WiFiDirect::*;

// Advertiser (host side)
let advertiser = WiFiDirectAdvertisementPublisher::new()?;
advertiser.Advertisement()?.ListenStateDiscoverability(
    WiFiDirectAdvertisementListenStateDiscoverability::Normal
)?;
advertiser.Start()?;

// Listener (accepts connections)
let listener = WiFiDirectConnectionListener::new()?;
listener.ConnectionRequested(&TypedEventHandler::new(|_, args| {
    let session = args.as_ref().unwrap().GetConnectionSession()?;
    // Handle connection
    Ok(())
}))?;
```

Wi-Fi Direct on Windows 10+ requires the user to be in the Administrators group or have the `WLAN_API` capability. May prompt UAC depending on configuration.

## System Tray

IRIS runs persistently in the Windows system tray. Users can quit the process but the default state is always-on relay.

```rust
use tauri::tray::{TrayIconBuilder, TrayIconEvent};

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let tray = TrayIconBuilder::with_id("iris-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("IRIS Mesh Network")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                if let Some(window) = tray.app_handle().get_window("main") {
                    window.show().unwrap();
                    window.set_focus().unwrap();
                }
            }
        })
        .build(app)?;
    Ok(())
}
```

## Key Storage

IRIS uses Windows Credential Manager via DPAPI (Data Protection API) for storing the node's private key:

```rust
use windows::Security::Cryptography::DataProtection::DataProtectionProvider;
use windows::Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream};

async fn protect_data(data: &[u8]) -> windows::core::Result<Vec<u8>> {
    let provider = DataProtectionProvider::CreateOverloadExplicit("LOCAL=user")?;
    // Encrypt data bound to current Windows user account
    let stream = InMemoryRandomAccessStream::new()?;
    // ... write and protect
}
```

For cross-session key persistence, store the DPAPI-protected blob in the Windows Credential Manager store:

```rust
// Use windows-credential-manager crate or direct WinAPI
// CredWrite / CredRead from Windows Credential Manager
```

## Firewall Configuration

IRIS may need to communicate on a fixed TCP port for relay connections. On first run, request Windows Firewall exception:

```rust
// Via netsh or Windows Firewall COM API
// Tauri can run this via a privileged installer or request elevated access
```

During development, temporary rule:
```powershell
netsh advfirewall firewall add rule name="IRIS Mesh" dir=in action=allow protocol=TCP localport=7788
```

## OTA Updates

Tauri 2.0 updater plugin:

```toml
[plugins.updater]
pubkey = "your-public-key"
endpoints = ["https://iris-releases.example.com/{{target}}/{{arch}}/{{current_version}}"]
```

Update check on startup, user prompted before installation. Signed with Ed25519 signing key (Tauri updater requirement).

## Build and Distribution

```powershell
# Development
cd desktop
npm run tauri dev

# Production build
npm run tauri build
# Output: src-tauri/target/release/bundle/msi/IRIS_x.y.z_x64_en-US.msi
# Also: src-tauri/target/release/bundle/nsis/IRIS_x.y.z_x64-setup.exe
```

Code signing: Windows binaries should be signed with an EV code signing certificate to avoid SmartScreen warnings. For initial development, unsigned is acceptable.
