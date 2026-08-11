# IRIS macOS Platform Documentation

## Platform Targets

- **Minimum macOS**: 11.0 (Big Sur)
- **Target macOS**: 14.0 (Sonoma)
- **Architecture**: Universal (x86_64 + arm64)
- **Framework**: Tauri 2.0
- **Rust toolchain**: stable (x86_64-apple-darwin + aarch64-apple-darwin)

## Architecture

macOS uses the same Tauri 2.0 architecture as Windows — a Rust backend with a React/TypeScript frontend in a webview. The BLE implementation uses CoreBluetooth, the same framework as iOS but with different background behavior characteristics.

### Stack

```
UI:             React 18 + TypeScript (WKWebView)
IPC:            Tauri invoke() — same as Windows
Rust backend:   Tauri 2.0 app + iris-core
BLE:            CoreBluetooth (CBCentralManager + CBPeripheralManager)
Local network:  Network.framework (NWBrowser + NWListener)
mDNS/Bonjour:  NWBrowser with Bonjour service type
Key storage:    macOS Keychain (Security framework)
Storage:        SQLite via rusqlite
Background:     macOS LaunchAgent or persistent foreground app
```

### Directory Structure

Same as Windows (`desktop/`) — both macOS and Windows share the Tauri app source. Platform-specific code is gated with `#[cfg(target_os = "macos")]` in Rust and `process.platform === 'darwin'` in TypeScript.

## CoreBluetooth on macOS

### Key Differences from iOS

macOS runs the app continuously without background suspension (unless user explicitly quits). This means:
- **CBCentralManager**: scans indefinitely without background constraints
- **CBPeripheralManager**: advertises indefinitely — no background suspension

macOS is a full BLE relay node with no foreground/background distinction for Bluetooth purposes.

### Entitlements and Permissions

macOS 10.15 Catalina+ requires Bluetooth permission. Entitlement required for Hardened Runtime:

**app.entitlements:**
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.device.bluetooth</key>
    <true/>
    <key>com.apple.security.network.client</key>
    <true/>
    <key>com.apple.security.network.server</key>
    <true/>
</dict>
</plist>
```

**Info.plist:**
```xml
<key>NSBluetoothAlwaysUsageDescription</key>
<string>IRIS uses Bluetooth to form a mesh network with nearby devices for offline communication.</string>
<key>NSLocalNetworkUsageDescription</key>
<string>IRIS uses the local network to discover and communicate with nearby IRIS relay nodes.</string>
```

### CoreBluetooth CBCentralManager

```rust
// On macOS, CoreBluetooth is accessed via objc2 or the corebluetooth crate
// Or bridged from a Swift helper XPC service
// Using swift-bridge approach:
```

For macOS Tauri app, CoreBluetooth can be invoked from Rust via a Swift helper using `swift-bridge` or `objc2`. Alternatively, a small Swift XPC service handles BLE and communicates with the Rust backend via Unix socket.

Pattern used: Swift BLE service process ↔ Unix domain socket ↔ Rust Tauri backend. This keeps the BLE code in Swift (familiar) while the protocol logic stays in Rust.

## Local Network Discovery (Bonjour/mDNS)

macOS has excellent Bonjour support via Network.framework. IRIS uses Bonjour to discover IRIS nodes on the local Wi-Fi network:

### NWBrowser (Discover)

```swift
// Swift helper service
let browser = NWBrowser(for: .bonjour(type: "_iris._tcp", domain: nil), using: .tcp)
browser.browseResultsChangedHandler = { results, changes in
    for result in results {
        if case .service(let name, _, _, _) = result.endpoint {
            // Found IRIS node: name = node ID hex
            notifyRustBackend(nodeId: name, endpoint: result.endpoint)
        }
    }
}
browser.start(queue: .main)
```

### NWListener (Advertise)

```swift
let parameters = NWParameters.tcp
parameters.includePeerToPeer = true

let listener = try! NWListener(using: parameters)
listener.service = NWListener.Service(type: "_iris._tcp")
listener.newConnectionHandler = { connection in
    handleIncomingConnection(connection)
}
listener.start(queue: .main)
```

Local network discovery gives IRIS a high-bandwidth transport path on the same Wi-Fi network — potentially 50-100MB/s vs BLE's ~20KB/s. Used for bulk message sync when nodes are on the same network.

## macOS as Gateway Node

macOS (typically a MacBook) is an excellent IRIS gateway node:
- **Ethernet**: wired Internet connection available
- **Wi-Fi**: wireless network connection
- **BLE**: always-on BLE relay (no background suspension)
- **Processing power**: routing computation runs fast
- **Storage**: large SSD — can store extensive message history

Gateway configuration: macOS IRIS app connects to IRIS relay server (if configured) and bridges between the relay server and the local BLE/Wi-Fi mesh. When a node in the BLE mesh needs to reach a node outside Bluetooth range, the macOS gateway forwards via Internet.

## Hardened Runtime

For App Store distribution and notarization, IRIS must enable Hardened Runtime:

**Tauri tauri.conf.json:**
```json
{
  "bundle": {
    "macOS": {
      "hardenedRuntime": true,
      "entitlements": "entitlements.plist",
      "signingIdentity": "Developer ID Application: IRIS Tech (XXXXXXXXXX)"
    }
  }
}
```

Hardened Runtime restrictions that affect IRIS:
- `com.apple.security.cs.allow-jit`: not needed (no JIT compilation)
- `com.apple.security.cs.disable-library-validation`: may be needed for loading Rust .dylib
- `com.apple.security.device.bluetooth`: required for CoreBluetooth

### Notarization

```bash
# After building .app bundle
xcrun notarytool submit IRIS.dmg \
    --apple-id developer@example.com \
    --team-id XXXXXXXXXX \
    --password @keychain:AC_PASSWORD \
    --wait
xcrun stapler staple IRIS.dmg
```

## macOS Keychain

Key storage via macOS Keychain using Security framework. Rust accesses Keychain via `security-framework` crate:

```toml
[target.'cfg(target_os = "macos")'.dependencies]
security-framework = "2.9"
```

```rust
use security_framework::item::{ItemClass, ItemSearchOptions, Reference, SearchResult};
use security_framework::passwords::{get_generic_password, set_generic_password};

fn store_private_key(key_bytes: &[u8]) -> Result<(), SecurityFrameworkError> {
    set_generic_password("app.iris.identity", "private-key", key_bytes)
}

fn load_private_key() -> Result<Vec<u8>, SecurityFrameworkError> {
    get_generic_password("app.iris.identity", "private-key")
}
```

Keys stored with `kSecAttrAccessibleWhenUnlockedThisDeviceOnly` are inaccessible when the Mac is locked — appropriate for private keys.

## Build and Distribution

### Universal Binary

```bash
# Add both targets
rustup target add x86_64-apple-darwin
rustup target add aarch64-apple-darwin

# Build universal
npm run tauri build -- --target universal-apple-darwin
# Output: src-tauri/target/universal-apple-darwin/release/bundle/macos/IRIS.app
# Also: IRIS_x.y.z_universal.dmg
```

### Code Signing

```bash
codesign --sign "Developer ID Application: IRIS Tech (XXXXXXXXXX)" \
    --entitlements entitlements.plist \
    --options runtime \
    --deep \
    IRIS.app
```

## Tauri-Specific macOS Configuration

```json
// tauri.conf.json (macOS section)
{
  "bundle": {
    "macOS": {
      "minimumSystemVersion": "11.0",
      "frameworks": [],
      "exceptionDomain": "",
      "signingIdentity": null,
      "providerShortName": null,
      "entitlements": "entitlements.plist"
    }
  }
}
```

## Testing

- Unit tests: same Rust tests as other platforms
- BLE integration: physical Mac with Bluetooth required
- Local network: requires two machines on same LAN or network namespace trick
- Notarization: tested in CI with Apple developer credentials
- Gateway scenario: Mac + 2 mobile phones — primary integration test target
