# Edge Architecture

## What Are Edge Nodes
Dedicated, always-on hardware nodes deployed in fixed locations to provide persistent mesh infrastructure. Unlike mobile phones (intermittent, battery-limited, OS-restricted background operations), edge nodes are designed for 24/7 operation with full transport capabilities.

Edge nodes serve three roles simultaneously:
1. **Mesh relay**: store-carry-forward for all messages in range
2. **Gateway**: bridge to Internet, LoRa backbone, or satellite
3. **Infrastructure anchor**: stable routing reference point for mobile nodes

## Reference Hardware: Raspberry Pi 4 / CM4

The Raspberry Pi 4 Model B is the primary reference hardware for IRIS edge nodes. Selected for: cost, availability in India, community support, GPIO for LoRa integration, and proven reliability in field deployments.

### Specification
```
CPU:       Broadcom BCM2711, Quad-core Cortex-A72 (ARM v8), 64-bit, 1.8GHz
RAM:       4GB LPDDR4-3200 SDRAM (2GB or 8GB variants also used)
Storage:   32GB-256GB microSD (Samsung Pro Endurance) or eMMC (CM4 with eMMC)
           Note: eMMC strongly preferred for reliability; microSD fails in high-write environments
Ethernet:  Gigabit Ethernet (true Gigabit via USB 3.0 bridge — not PCIe)
Wi-Fi:     Dual-band (2.4GHz and 5GHz) 802.11ac (Wi-Fi 5)
Bluetooth: Bluetooth 5.0, BLE
USB:       2× USB 3.0, 2× USB 2.0 (for LoRa module, GPS, etc.)
GPIO:      40-pin GPIO header (SPI for LoRa, I2C for sensors)
Video:     2× micro HDMI (not used in headless deployment)
Power:     5V 3A via USB-C (15W typical), 5V via GPIO (for PoE HATs)
OS:        Raspberry Pi OS Lite (64-bit, Debian Bookworm base), headless

Pricing (India, 2024):
  RPi 4 4GB: ₹6,500-8,000
  RPi CM4 4GB (no eMMC): ₹7,500-9,500
  CM4 carrier board: ₹2,000-4,000
  Case + cooling: ₹500-1,500
  MicroSD (high endurance): ₹800-1,500
  Power supply: ₹500-1,000
  Total RPi 4 node (no LoRa): ₹8,300-12,000
```

### Alternative Hardware
- **Orange Pi 5**: higher performance, similar cost, less community support
- **Radxa Rock 5A**: PCIe slot useful for LoRa mPCIe module
- **BeagleBone Black**: more GPIO, less RAM, older
- **Custom PCB**: for high-volume production deployment (future)

## LoRa Integration

LoRa extends IRIS mesh range from BLE/Wi-Fi's 50-200m to 5-15km in open terrain.

### LoRa Module Options

| Module | Type | Range | Channels | Cost (India) | Interface |
|--------|------|-------|---------|-------------|----------|
| RAK Wireless RAK2287 | LoRaWAN concentrator | 10-15km | 8+ | ₹12,000-18,000 | mPCIe/SPI |
| Dragino PG1301 HAT | LoRaWAN concentrator | 10-15km | 8 | ₹8,000-12,000 | SPI (HAT) |
| RAK831 | LoRaWAN concentrator | 10-15km | 8 | ₹6,000-10,000 | SPI |
| SX1276 breakout | Single-channel | 2-5km | 1 | ₹400-800 | SPI |
| E22-900T30D | Single-channel module | 5-10km | 1 | ₹600-1,200 | UART |

**Recommended for production**: RAK2287 (8-channel concentrator) for maximum throughput and multi-node coverage.
**Recommended for testing/development**: SX1276 breakout board (low cost, adequate for experiments).

### LoRa Configuration for India
```
Frequency band: 865-867 MHz (IN865, ISM band, license-free)
Spreading factor: SF7 (default, 5.47kbps), SF12 (max range, 293bps)
Bandwidth: 125kHz (default)
Coding rate: 4/5
Max transmit power: 14dBm EIRP (India regulation)
Duty cycle: 1% (max 36 seconds transmit per hour per channel)
Antenna: External fiberglass, 3dBi or 5dBi, N-type connector

Key design constraint: duty cycle limits throughput severely.
At SF7, 255-byte packet: ~1.8 seconds airtime.
At 1% duty cycle: max ~20 packets/hour on single channel.
8-channel concentrator: max ~160 packets/hour total.
Design implication: LoRa is for P0-P3 only. P4+ must use BLE/Wi-Fi or Internet.
```

### Antenna Placement
- Antenna must be external (inside building attenuates signal 10-30dB)
- Recommended: N-type panel or omnidirectional antenna, rooftop mount
- Cable: LMR-200 or LMR-400, keep runs short (loss: ~0.7dB/m at 900MHz for LMR-200)
- Lightning protection: gas discharge arrester between antenna and module

## Power System for Disaster Resilience

IRIS edge nodes are deployed for disaster scenarios where mains power may be unavailable for days or weeks.

### Solar + Battery System (Reference Design)

```
┌─────────────────────────────────────────────────────────────────┐
│                    Solar Power System                           │
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌─────────────────┐  │
│  │ Solar Panel  │───►│   MPPT Charge│───►│  LiFePO4       │  │
│  │ 20W (1 panel)│    │   Controller │    │  Battery       │  │
│  │ or 40W (2)   │    │ Victron 75/15│    │  20Ah 12V      │  │
│  └──────────────┘    └──────────────┘    └────────┬────────┘  │
│                                                    │           │
│                            ┌───────────────────────┘           │
│                            │ 12V DC                            │
│                            ▼                                   │
│                    ┌───────────────┐                           │
│                    │ DC-DC 12V→5V  │                           │
│                    │ 3A converter  │                           │
│                    └───────┬───────┘                           │
│                            │ 5V, 3A                            │
│                            ▼                                   │
│                    ┌───────────────┐                           │
│                    │ Raspberry Pi 4│                           │
│                    │ + LoRa module │                           │
│                    └───────────────┘                           │
└─────────────────────────────────────────────────────────────────┘

Power budget:
  RPi 4 idle: ~3W
  RPi 4 under load (Wi-Fi + BLE active): ~6W
  LoRa transmit: ~1W peak, ~0.2W average
  Total system: ~7W average
  
Battery capacity: 20Ah × 12V = 240Wh
  Runtime at 7W: 240Wh / 7W ≈ 34 hours without sun
  Recharge time: 40W panel → ~6 hours full sun
  
Annual sun hours (India, various locations):
  Rajasthan, Gujarat: 6-7 hours/day average
  Northeast India: 3-4 hours/day average
  
Recommended: 40W panel for northeast India or cloudy regions
Recommended: 40Ah battery (80Wh, 68h runtime) for extended cloudy periods
```

### UPS for Urban Nodes
For urban nodes with mains power:
- UPS HAT for Raspberry Pi (e.g., Waveshare UPS HAT, PiJuice)
- 3,000-5,000mAh LiPo battery
- Runtime on battery: 3-5 hours (bridge mains outages)
- More cost-effective than full solar system where mains is usually available

### Power Monitoring
Edge node monitors its own power state:
- Battery voltage via ADC (GPIO or I2C ADC module)
- Solar panel current (shunt resistor + ADC)
- Triggers low-battery message relay behavior at configurable threshold
- Reports power stats via local health API and telemetry

## Software Stack on Edge Node

### Operating System
```
OS: Raspberry Pi OS Lite (64-bit, Debian Bookworm)
  - No desktop environment (headless)
  - Automatic security updates enabled
  - Unattended-upgrades configured for security patches
  - Do NOT auto-update IRIS daemon (controlled separately)
```

### IRIS Daemon
```
Binary: iris-relay (Rust, compiled for aarch64)
Service: systemd unit file: /etc/systemd/system/iris-relay.service
Config: /etc/iris/config.yaml
Logs: journald (retained 7 days)
Data: /var/lib/iris/ (message store, routing state, identity)
```

Systemd unit:
```ini
[Unit]
Description=IRIS Relay Daemon
After=network.target bluetooth.target
Wants=network.target

[Service]
Type=simple
User=iris
Group=iris
ExecStart=/usr/local/bin/iris-relay --config /etc/iris/config.yaml
Restart=always
RestartSec=5
WatchdogSec=60

[Install]
WantedBy=multi-user.target
```

### Watchdog
- Hardware watchdog: `/dev/watchdog` (RPi has built-in)
- Configuration: `dtparam=watchdog=on` in `/boot/config.txt`
- Timeout: 15 seconds (reboot if daemon hangs)
- IRIS daemon: pets watchdog every 5 seconds

### Remote Management
When Internet available:
- SSH: key-based authentication only (`PasswordAuthentication no`)
- SSH port: non-standard (configurable, reduces bot scanning)
- Firewall: nftables, only IRIS port (47221) and SSH port open
- Monitoring: Prometheus metrics endpoint (local network) + push when Internet available

When Internet unavailable:
- BLE config interface: phone app can connect to configure and view status
- Local UART console (via GPIO pins, emergency access)

### OTA Updates
- Signed firmware updates via A/B partition scheme
- Update served via Internet (when available) or USB
- Signature: Ed25519, signed by IRIS maintainer keys
- Rollback: if new firmware fails health check within 60 minutes, roll back automatically

## Deployment Scenarios

### Scenario 1: Urban Rooftop Node (Community Building)
```
Location:     Rooftop of panchayat building, school, or hospital
Power:        Mains + UPS HAT (3h battery)
Ethernet:     Cat6 to building router (Internet gateway)
Antennas:     BLE: PCB antenna (internal, sufficient for rooftop)
              Wi-Fi: omnidirectional, external, 3dBi
              LoRa: fiberglass omnidirectional, 5dBi, 5m mast
Coverage:     BLE: ~50m radius (urban, obstructions)
              Wi-Fi: ~150m radius
              LoRa: 2-5km urban, 5-15km open terrain
Role:         Internet gateway + LoRa relay + BLE/Wi-Fi local hub
Setup time:   2-4 hours (hardware + software)
```

### Scenario 2: Rural Solar Village Node
```
Location:     Rooftop of gram panchayat, post office, or health sub-center
Power:        20W solar + 20Ah LiFePO4 battery (~34h autonomy)
Connectivity: LoRa to block-level node (3-10km)
              BLE + Wi-Fi for village coverage
Antennas:     LoRa: 5dBi fiberglass on 3m pole
              Wi-Fi: omnidirectional 3dBi
Coverage:     Village radius ~200m via BLE/Wi-Fi
Role:         Village mesh hub + LoRa relay to block level
Bandwidth:    LoRa: ~20 P0-P3 messages/hour (duty cycle limited)
              BLE: unlimited within duty cycle constraints
Setup time:   4-6 hours (solar mounting + software)
```

### Scenario 3: Disaster Pre-Positioned Node (Rapid Deployment)
```
Package:      Pelican case with RPi 4, LoRa module, solar panel, battery
Power:        20Ah battery (34h standalone) + included 20W solar panel
Antennas:     Collapsible LoRa antenna, foldable solar panel
Deployment:   Unpack, power on, deploy antenna — operational in 10 minutes
Target users: NDRF/SDRF teams, district disaster management authority
Role:         Instant mesh infrastructure in disaster-affected area
Storage:      256GB microSD (large message buffer for DTN)
```

### Scenario 4: Vehicle Mobile Node
```
Vehicle:      NDRF/SDRF vehicle, supply convoy truck
Power:        12V vehicle power + 40Ah LiFePO4 (standalone operation)
Connectivity: RPi 4 + LoRa + BLE + Wi-Fi + 4G router (dual SIM)
Antennas:     Roof-mounted omnidirectional LoRa + Wi-Fi
Role:         Mobile DTN relay (store-carry-forward across partitions)
              Internet gateway when cellular available
              Bridge between disconnected communities during convoy
Storage:      512GB SSD (via USB 3.0) for large message buffer
```

## Edge Node Administration

### Local Web Dashboard
Available at `http://192.168.x.x:8080` on local network:
- Neighbor list (nodes in range, last seen, transport)
- Message queue depth by priority
- Storage utilization
- Power status (battery level, solar input, power mode)
- Transport status (BLE, Wi-Fi, LoRa: up/down, packet counts)
- Recent log entries

### CLI Tools
```bash
iris-ctl status              # Overall node status
iris-ctl neighbors           # Current neighbor table
iris-ctl channels            # Transport/channel status
iris-ctl queue               # Message queue statistics
iris-ctl storage             # Storage utilization breakdown
iris-ctl logs [--tail 100]   # Recent log entries
iris-ctl config show         # Current configuration
iris-ctl config set key=val  # Update configuration
iris-ctl identity             # Show node identity (node_id, type)
iris-ctl update check        # Check for OTA updates
iris-ctl update apply        # Apply pending OTA update
```

### BLE Configuration Interface
For field configuration without laptop:
- BLE service UUID: "5f4b5401-3c4b-4952-4953-000000000002" // IRIS-CONFIG-v1
- Authenticated: requires pairing with node's configuration PIN (displayed on node or set during setup)
- Operations: view status, configure relay settings, add contact schedules
- Implemented as IRIS phone app feature (not separate app)

## Security for Edge Nodes

### Storage Encryption
- Full disk encryption: LUKS2 with AES-256-XTS
- Key stored in TPM 2.0 (if RPi variant has TPM) or hardware security module
- Without TPM: key derived from hardware identifier + operator passphrase
- Implication: physical theft of SD card does not reveal stored messages

### Network Security
```
Firewall rules (nftables):
  ACCEPT input on loopback
  ACCEPT input on established/related
  ACCEPT input on IRIS port (47221) from any       # IRIS protocol, authenticated
  ACCEPT input on SSH port from operator subnet     # Management access
  ACCEPT input on ICMP (ping) from local subnet     # Diagnostics
  DROP all other input
  ACCEPT all output
```

### Physical Security
- Tamper-evident case: epoxy or tamper-evident screws for critical installations
- Asset tagging: GPS tracker in pre-positioned disaster kits
- Logging: all configuration changes logged with timestamp and source (BLE or SSH)

### Node Identity
On edge nodes, Ed25519 key stored in:
- LUKS-encrypted partition (software-only security, no TEE equivalent)
- Optionally: dedicated HSM module via I2C/SPI (Microchip ATECC608, ~₹200)
- Recommendation: ATECC608 for production deployments (hardware-backed key storage)
- Limitation: no equivalent of Android Keystore / iOS Secure Enclave for general RPi hardware

### SSH Hardening
```
# /etc/ssh/sshd_config
PasswordAuthentication no
PubkeyAuthentication yes
PermitRootLogin no
AllowUsers iris-admin
MaxAuthTries 3
LoginGraceTime 30
ClientAliveInterval 300
ClientAliveCountMax 2
```
