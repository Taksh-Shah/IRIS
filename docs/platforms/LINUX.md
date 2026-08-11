# IRIS Linux Platform Documentation

## Platform Targets

- **Primary**: Ubuntu 20.04 LTS, Ubuntu 22.04 LTS
- **Secondary**: Debian 11 (Bullseye), Debian 12 (Bookworm)
- **Edge node**: Raspberry Pi OS (64-bit, based on Debian 11/12)
- **Architecture**: x86_64 (desktop), aarch64 (Raspberry Pi 4/5)

## Deployment Modes

Linux supports two deployment modes:

### 1. Desktop App (Tauri)
Same architecture as Windows/macOS. Tauri app with React frontend. Suitable for Linux workstations used as gateway nodes with a user interface.

### 2. Headless Daemon (`iris-relay`)
A standalone Rust binary with no UI. Primary deployment for Raspberry Pi edge nodes and server-based relay points. Managed via systemd. Configured via TOML file. Monitored via Prometheus metrics endpoint.

```
iris-relay --config /etc/iris/relay.toml
```

## Architecture (Headless)

```
iris-relay binary
├── iris-core (all protocol logic)
├── BlueZ BLE transport (via btleplug or bluer)
├── TCP transport (for LAN relay and gateway)
├── LoRa transport (optional, via serial/SPI)
├── Prometheus metrics endpoint (:9090/metrics)
└── gRPC control API (:7789) for iris-ctl management
```

## BLE via BlueZ

Linux BLE is provided by the BlueZ stack via D-Bus.

### System Requirements

```bash
# Install BlueZ
sudo apt install bluez bluetooth

# Verify adapter
hciconfig -a

# Check D-Bus service
systemctl status bluetooth
```

### Rust BLE Crates

**Option A: `btleplug`** — cross-platform BLE crate (Android/iOS/Windows/macOS/Linux). Abstracts BlueZ on Linux. Production-ready.

```toml
[target.'cfg(target_os = "linux")'.dependencies]
btleplug = "0.11"
```

```rust
use btleplug::api::{Central, Manager as _, Peripheral, ScanFilter};
use btleplug::platform::Manager;

async fn scan_for_iris_nodes() -> anyhow::Result<()> {
    let manager = Manager::new().await?;
    let adapters = manager.adapters().await?;
    let adapter = adapters.into_iter().next()
        .ok_or_else(|| anyhow::anyhow!("No Bluetooth adapter found"))?;

    adapter.start_scan(ScanFilter {
        services: vec![IRIS_SERVICE_UUID],
    }).await?;

    tokio::time::sleep(Duration::from_secs(10)).await;

    let peripherals = adapter.peripherals().await?;
    for peripheral in peripherals {
        let properties = peripheral.properties().await?.unwrap_or_default();
        tracing::info!(
            addr = %properties.address,
            rssi = ?properties.rssi,
            "Found IRIS node"
        );
    }
    Ok(())
}
```

**Option B: `bluer`** — async Rust BlueZ D-Bus library. Lower level than btleplug, more control, async-native.

```toml
[target.'cfg(target_os = "linux")'.dependencies]
bluer = { version = "0.17", features = ["full"] }
```

IRIS uses `btleplug` for initial implementation (cross-platform code reuse) with `bluer` as fallback for features btleplug does not expose.

### D-Bus Permissions

The user running `iris-relay` must be in the `bluetooth` group:

```bash
sudo usermod -aG bluetooth $USER
# Or for system service:
# Add to systemd service: SupplementaryGroups=bluetooth
```

Alternatively, grant D-Bus policy for the iris user in `/etc/dbus-1/system.d/iris-bluetooth.conf`:

```xml
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
    "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
    <policy user="iris">
        <allow send_destination="org.bluez"/>
        <allow receive_sender="org.bluez"/>
    </policy>
</busconfig>
```

## Wi-Fi Direct (P2P)

Wi-Fi Direct on Linux uses wpa_supplicant with P2P support:

```bash
# Check wpa_supplicant supports P2P
wpa_supplicant -v | grep P2P

# Typical wpa_supplicant configuration for P2P
wpa_cli p2p_find
wpa_cli p2p_connect <mac_addr> pbc
```

Rust integration via `wpa-ctrl` crate or direct wpa_supplicant Unix socket protocol. For Raspberry Pi edge nodes, Wi-Fi Direct provides faster bulk transfer than BLE.

## NetworkManager Integration

For desktop deployments, NetworkManager manages network interfaces. IRIS integrates via D-Bus:

```rust
// Monitor network state changes via NetworkManager D-Bus
// Detect when new network interface comes up (Wi-Fi connected)
// Trigger gateway mode when Internet becomes available
```

## Systemd Service

### `/etc/systemd/system/iris-relay.service`

```ini
[Unit]
Description=IRIS Resilient Communication Relay
After=network.target bluetooth.target
Wants=bluetooth.target
Documentation=https://iris-project.example.com/docs/linux

[Service]
Type=notify
User=iris
Group=iris
SupplementaryGroups=bluetooth
ExecStart=/usr/bin/iris-relay --config /etc/iris/relay.toml
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=10s
WatchdogSec=60s
RuntimeDirectory=iris
StateDirectory=iris
LogsDirectory=iris
ConfigurationDirectory=iris

# Capabilities for raw network access if needed
# AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW
# CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW

# Resource limits
LimitNOFILE=65536
MemoryMax=512M

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable iris-relay
sudo systemctl start iris-relay
sudo systemctl status iris-relay
```

### Zero-Downtime Upgrade

```ini
# Add to service file for zero-downtime:
ExecStartPost=/usr/bin/iris-relay-healthcheck
TimeoutStartSec=30

# Keep old process running until new one passes health check
# iris-relay accepts SIGTERM gracefully, finishes in-flight operations
```

## Raspberry Pi Edge Node

Raspberry Pi 4/5 is the primary IRIS edge node hardware. Typical deployment: pre-configured Pi placed at evacuation centers, fire stations, or elevated locations.

### Hardware Configuration

```
Raspberry Pi 4 (4GB RAM recommended)
├── Built-in: BCM43455 — BLE 5.0 + Wi-Fi 5 (2.4GHz + 5GHz)
├── Optional: LoRa HAT (RAK2287, WM1302, or similar)
├── Optional: External BLE antenna (SMA connector, improves range)
└── Power: PoE HAT or USB-C, UPS HAT for battery backup
```

### Cross-Compilation

From x86_64 Linux (CI):

```bash
# Install cross-compilation toolchain
rustup target add aarch64-unknown-linux-gnu
sudo apt install gcc-aarch64-linux-gnu

# .cargo/config.toml
[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"

# Build
cargo build --target aarch64-unknown-linux-gnu --release -p iris-relay
```

Or use the `cross` tool (Docker-based):
```bash
cargo install cross
cross build --target aarch64-unknown-linux-gnu --release
```

### Raspberry Pi OS Setup

```bash
# On Pi: enable Bluetooth
sudo systemctl enable bluetooth
sudo systemctl start bluetooth

# Install IRIS
sudo dpkg -i iris-relay_x.y.z_arm64.deb
# Or via apt repository:
# sudo apt install iris-relay

# Configure
sudo nano /etc/iris/relay.toml

# Start
sudo systemctl enable iris-relay
sudo systemctl start iris-relay
```

### Headless Configuration via BLE

For field deployment without keyboard/monitor, IRIS relay can be configured via BLE from a phone:
1. Pi starts with default config + BLE config mode (special service UUID)
2. Admin opens IRIS app → Node Management → Configure Relay
3. App connects via BLE, pushes configuration (channel key, node role, etc.)
4. Pi saves config, restarts relay service

## Network Routing / Gateway

Linux has full IP routing capability. When IRIS relay is on a Pi with both Internet (4G/Ethernet) and BLE mesh:

```bash
# Enable IP forwarding
sudo sysctl -w net.ipv4.ip_forward=1
echo "net.ipv4.ip_forward=1" | sudo tee -a /etc/sysctl.conf

# iptables for NAT (mesh nodes → Internet)
sudo iptables -t nat -A POSTROUTING -o eth0 -j MASQUERADE
sudo iptables -A FORWARD -i wlan0 -o eth0 -j ACCEPT
sudo iptables -A FORWARD -i eth0 -o wlan0 -m state --state RELATED,ESTABLISHED -j ACCEPT
```

## Packaging

### .deb Package

```bash
# Build with cargo-deb
cargo install cargo-deb
cargo deb -p iris-relay

# Output: target/debian/iris-relay_x.y.z_amd64.deb
# And: target/debian/iris-relay_x.y.z_arm64.deb (cross-compiled)
```

`Cargo.toml` metadata for cargo-deb:
```toml
[package.metadata.deb]
maintainer = "IRIS Team <iris@example.com>"
copyright = "2025 IRIS Project"
license-file = ["LICENSE", "4"]
extended-description = "IRIS resilient mesh relay daemon"
depends = "$auto, bluez, libdbus-1-3"
section = "net"
priority = "optional"
assets = [
    ["target/release/iris-relay", "usr/bin/", "755"],
    ["config/relay.toml.example", "etc/iris/relay.toml", "644"],
    ["systemd/iris-relay.service", "lib/systemd/system/", "644"],
]
maintainer-scripts = "debian/"
systemd-units = { enable = true }
```

### AppImage

For portable distribution (no installation required):

```bash
cargo install cargo-appimage
cargo appimage -p iris-relay
```

## Raw Socket Capabilities

If IRIS needs raw socket access (e.g., for custom Wi-Fi management):

```bash
# Grant capability to binary without running as root
sudo setcap cap_net_admin+eip /usr/bin/iris-relay
sudo setcap cap_net_raw+eip /usr/bin/iris-relay

# Verify
getcap /usr/bin/iris-relay
```

This replaces the need to run as root while still allowing low-level network operations.

## Logging

Systemd journal integration via `tracing` + `tracing-journald`:

```toml
[dependencies]
tracing-journald = "0.3"
```

```rust
tracing_journald::layer()
// Logs appear in: journalctl -u iris-relay -f
```

For file logging:
```bash
# iris-relay logs to /var/log/iris/relay.log by default
# Logrotate config: /etc/logrotate.d/iris
```
