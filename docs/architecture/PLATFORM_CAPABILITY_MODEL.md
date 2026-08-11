# Platform Capability Model

## Purpose

This document establishes the **first-class capability model** for IRIS node behavior across platforms. It supersedes any architecture document that treats platform differences as footnotes.

## Core Principle

**Every phone is NOT always a relay.** The architecture must never assume that a device has static, always-available relay capability. Relay capability is a function of multiple dynamic states:

```
Current relay capability =
    Device hardware capability
  × Current OS state
  × Current permission state
  × Current battery state
  × Current radio state
  × Current background execution restrictions
```

Each factor can independently reduce capability to zero. The architecture must model this explicitly.

---

## Capability Dimensions

### Dimension 1: Transport Availability

Each transport is independently available or unavailable:

| Transport | Android | iOS | Desktop (Win/Mac/Linux) |
|-----------|---------|-----|------------------------|
| BLE Advertising | ✓ (API 21+) | ✓ (foreground only*) | Limited (platform dependent) |
| BLE Scanning | ✓ | ✓ (foreground only*) | Limited |
| Wi-Fi Aware (NAN) | ✓ (API 26+, hardware dependent) | ✗ (not supported) | ✗ |
| Wi-Fi Direct | ✓ | Limited (via Network.framework) | ✓ |
| Cellular | ✓ | ✓ | via USB tether |
| Internet (TCP) | ✓ | ✓ | ✓ |
| LoRa (via BLE bridge) | ✓ | ✓ (foreground only) | ✓ |

*iOS BLE in background: device can receive Central connections and respond to GATT reads, but **advertising stops** and **active scanning stops** when the app enters background.

### Dimension 2: Background Execution

**This is the most critical architectural constraint and must not be treated as a footnote.**

| Behavior | Android | iOS |
|----------|---------|-----|
| Foreground service allowed | ✓ (with notification) | ✗ (no equivalent) |
| Background BLE advertising | ✓ (via foreground service) | ✗ — stops when app backgrounds |
| Background BLE scanning | ✓ (via foreground service) | Limited (brief resumed on connection) |
| Background Wi-Fi Aware | ✓ (via foreground service) | ✗ (not available) |
| Background execution time | Unlimited (foreground service) | ~30 seconds per background task |
| Manufacturer-specific kill | High (Xiaomi, Huawei, Samsung) | N/A (Apple enforces uniformly) |
| Doze mode | Android 6+ — batch wakeups | N/A |

**Apple official documentation (current):**
> When an app using Multipeer Connectivity moves to the background, advertising and browsing stop, and any open sessions are disconnected.

**Architecture implication:** iOS CANNOT be treated as an equivalent relay to Android. An iOS device acting as a relay will stop relaying the moment the user switches apps or the screen locks (unless actively on charger in special background modes). This is not a limitation that can be worked around — it is a fundamental iOS system constraint.

### Dimension 3: Android Wi-Fi Aware Dynamic Availability

Wi-Fi Aware is NOT a fixed device property. Its availability is:
- **Hardware dependent**: the device chipset must support Wi-Fi Aware
- **OS dependent**: Android 8.0 (API 26) minimum
- **State dependent**: Wi-Fi must be enabled, Wi-Fi Aware must not be disabled by system or power management
- **Concurrent usage dependent**: may be unavailable if Wi-Fi is in use for AP connection in certain modes

```kotlin
// Check Wi-Fi Aware availability — do this at runtime, not once at startup
fun checkWifiAwareAvailability(context: Context): WifiAwareState {
    val wifiAwareManager = context.getSystemService(WifiAwareManager::class.java)
        ?: return WifiAwareState.NOT_SUPPORTED  // API not available
    
    return if (wifiAwareManager.isAvailable) {
        WifiAwareState.AVAILABLE
    } else {
        WifiAwareState.TEMPORARILY_UNAVAILABLE  // May come back
    }
}
```

The `WifiAwareManager.isAvailable` state can change at runtime. IRIS must listen for `ACTION_WIFI_AWARE_STATE_CHANGED` broadcasts and update its transport capability table dynamically.

---

## Node Capability State Machine

Every node has a capability state that changes dynamically:

```rust
pub struct NodeCapability {
    /// Current transports that are active and usable
    pub active_transports: Vec<TransportId>,
    /// Current transports that exist but are temporarily unavailable
    pub inactive_transports: Vec<TransportId>,
    /// Whether this node can currently act as a relay
    pub relay_capable: bool,
    /// Estimated time relay capability will remain (None = indefinite)
    pub relay_duration_estimate: Option<Duration>,
    /// Battery level affecting capability
    pub battery_mode: BatteryMode,
    /// OS background state
    pub foreground: bool,
}

impl NodeCapability {
    pub fn effective_relay_capability(&self) -> RelayLevel {
        if !self.foreground && self.platform == Platform::iOS {
            return RelayLevel::None;  // iOS background = no relay
        }
        if self.battery_mode == BatteryMode::EmergencyOnly {
            return RelayLevel::P0Only;  // Only emergency relay
        }
        if self.active_transports.is_empty() {
            return RelayLevel::None;
        }
        RelayLevel::Full
    }
}
```

---

## Platform Capability Tiers

IRIS defines four node tiers based on capability:

### Tier 1: Full Relay Node
**Devices**: Android phone (foreground service active), Linux edge server, Raspberry Pi
- Background execution: unlimited
- All transports available
- Can relay for extended periods
- Used in: permanent edge deployments, active disaster responders

### Tier 2: Opportunistic Relay Node  
**Devices**: Android phone (background, Doze mode), Windows/macOS laptop
- Background execution: intermittent (Doze wakeups, user interaction)
- Most transports available when awake
- Relay when foreground or Doze wakeup windows
- Used in: bystanders, responders not actively using app

### Tier 3: Contact-Opportunistic Node
**Devices**: iPhone (background state)
- Background execution: severely limited
- BLE advertising STOPS in background
- Can only relay when app is in foreground or during brief background wakeup
- Best used as: message originator and direct recipient, not relay
- **Architecture must not count on iPhone as mesh relay**

### Tier 4: Emergency-Only Node
**Devices**: Any phone at <5% battery
- Only P0 SOS transmission capability
- No relay
- Minimal UI

---

## Capability Advertisement

Every node advertises its current capability tier in BLE advertisements and during sync handshake:

```
CapabilityAdvertisement {
    tier: u2,                    // 0=Tier4, 1=Tier3, 2=Tier2, 3=Tier1
    transports_bitmap: u8,       // Bit per transport type
    battery_level: u4,           // 0-15 (0=<7%, 15=100%)
    foreground: bool,
    platform: u4,                // ANDROID, IOS, WINDOWS, MACOS, LINUX, RPI
}
```

This allows routing decisions to prefer Tier 1/2 nodes over Tier 3 nodes as relays.

---

## Architecture Implications

1. **The routing algorithm must weight relay candidates by capability tier.** PRoPHET delivery probability must be adjusted down for Tier 3 nodes (iOS) that may not be available for relay.

2. **The simulation model must use platform-accurate availability models.** An iOS node in simulation must model the foreground/background state transitions.

3. **The ANDROID-001 and IOS-001 graph nodes must have explicit acceptance criteria for background behavior.** "Works in background" is not an acceptance criterion — the actual behavior must be specified and tested.

4. **Documentation must never say "any device can be a relay" without the capability qualification.**

---

## Known Limitations

- Android manufacturers (Xiaomi, Huawei, Samsung) impose aggressive battery optimization that kills background services even with foreground service active. IRIS must test on major Indian market devices specifically.
- iOS background BLE behavior can change with iOS updates. This capability model must be revalidated on every major iOS version.
- Wi-Fi Aware availability varies across Android device families in ways not documented by Google.

---

## References

- Android background optimization: https://developer.android.com/topic/performance/background-optimization
- Apple Multipeer Connectivity background behavior: https://developer.apple.com/documentation/multipeerconnectivity
- Android Wi-Fi Aware: https://developer.android.com/develop/connectivity/wifi/wifi-aware
- `docs/platforms/ANDROID.md` — Android-specific implementation
- `docs/platforms/IOS.md` — iOS-specific constraints and workarounds
