# Battery — Power Budget and Drain Analysis

**Component:** All IRIS apps (Android, iOS, embedded)
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

Battery life is a first-class design constraint for IRIS. In a disaster scenario, grid power may be unavailable for days. IRIS must operate for at least 24 hours on a typical smartphone battery (3,500–5,000 mAh) while providing useful mesh networking.

**Primary target:** 24-hour operation at ≤ 20% battery drain in typical disaster scenario (background relay, BLE advertising, periodic LoRa if hardware present).

**P0 target:** < 2% battery drain per SOS transmission sequence (send + await ack).

---

## 2. Operating Modes

IRIS defines four power modes. Mode transitions are automatic based on battery level and user settings. The mode is shown in the app status bar.

| Mode | Trigger | Transports | Routing scope | Target drain/hour |
|---|---|---|---|---|
| **Full** | Battery ≥ 30% | BLE + Wi-Fi + LoRa + Cellular | All priorities | ≤ 1.5% (1,500 mAh battery / 24h = 62.5 mAh/h) |
| **Reduced** | Battery 15–30% | BLE + LoRa | P0–P3 only | ≤ 0.8% /h |
| **Emergency** | Battery 5–15% | LoRa only (or BLE only) | P0–P1 only | ≤ 0.3% /h |
| **SOS-only** | Battery < 5% | Lowest-power transport | P0 only | ≤ 0.15% /h |

---

## 3. Per-Transport Power Draw

### 3.1 BLE 5.x

| State | Current (mA) | Typical duty cycle | Avg current |
|---|---|---|---|
| Advertising @ 1 Hz | 2.0 | 100% (continuous) | 2.0 mA |
| Advertising @ 10 Hz | 8.0 | 10% (discovery phase) | 0.8 mA |
| Active scanning | 20.0 | 5–15% | 1.0–3.0 mA |
| Connected (low throughput) | 5.0 | 0–10% | 0.0–0.5 mA |
| Connected (data transfer) | 30.0 | 0–5% | 0.0–1.5 mA |
| Idle (no BLE activity) | 0.1 | — | 0.1 mA |

BLE advertising at 1 Hz is the baseline; IRIS always advertises while in Full or Reduced mode.

### 3.2 Wi-Fi Direct

| State | Current (mA) | Notes |
|---|---|---|
| P2P listening | 15 | Wi-Fi chip awake, no data |
| P2P connection setup | 80 | Negotiation phase, < 5s |
| Data transfer | 100 | Sustained throughput |
| Idle (Wi-Fi off) | 0 | Wi-Fi disabled in Emergency mode |

Wi-Fi Direct is disabled in Emergency and SOS-only modes. In Full and Reduced modes, IRIS enables Wi-Fi only when a peer is already discovered via BLE (to avoid continuous Wi-Fi scan overhead).

### 3.3 LoRa

| State | Current (mA) | Notes |
|---|---|---|
| Receive mode (continuous) | 12 | SX1262 RX continuous |
| Transmit @ 14 dBm | 120 | WPC limit, SF9 144ms air time |
| Transmit @ 10 dBm | 80 | Reduced power for short range |
| Sleep | 0.002 | SX1262 deep sleep |

LoRa duty cycle limit (1%) constrains TX to ≤ 36 seconds per hour. At 144 ms per packet (SF9, 64 bytes), this is a maximum of ~250 TX packets per hour.

Average LoRa power (relay node, moderate traffic):
```
I_avg_lora = 0.5 × 12 (RX 50% duty) + 0.01 × 120 (TX 1% max duty)
           = 6 + 1.2 = 7.2 mA
```

### 3.4 Cellular (LTE)

| State | Current (mA) |
|---|---|
| Connected, idle | 10 |
| Active data transfer | 200–400 |
| Background (Doze) | 1 |

Cellular is treated as opportunistic in IRIS — used when available but not depended on. In Emergency mode, cellular is used for P0 if available but not kept alive proactively.

---

## 4. Total Power Budget

### 4.1 Full Mode (Android, 4,000 mAh)

| Component | Current (mA) |
|---|---|
| SoC (Snapdragon 680, background) | 50 |
| Screen (off) | 0 |
| BLE advertising | 2.0 |
| BLE scan (5% duty) | 1.0 |
| Wi-Fi (listening, no P2P) | 5.0 |
| LoRa RX (50% duty) | 6.0 |
| LoRa TX (1% duty) | 1.2 |
| **Total** | **65.2 mA** |

```
Battery lifetime = 4000 mAh / 65.2 mA ≈ 61.3 hours
Daily drain = 65.2 × 24 / 4000 = 39%
```

**Status: Exceeds 20% target.** Mitigation strategies:

1. Reduce BLE scan duty to 2% (not 5%): saves 0.4 mA
2. Reduce LoRa RX to 20% duty (use aggressive sleep): saves 3.6 mA
3. Disable Wi-Fi when no P2P peer discovered in 10 minutes: saves 5.0 mA
4. Reduce SoC base from 50 to 35 mA using Android WorkManager scheduling: saves 15 mA

With all mitigations: Total ≈ 41 mA → Daily drain = 25%. Closer to target; further optimization needed.

### 4.2 Emergency Mode (LoRa only)

| Component | Current (mA) |
|---|---|
| SoC (minimal) | 20 |
| BLE advertising (1 Hz) | 2.0 |
| LoRa RX (20% duty) | 2.4 |
| LoRa TX (1% duty max) | 1.2 |
| **Total** | **25.6 mA** |

```
Battery lifetime = 4000 mAh / 25.6 mA ≈ 156 hours
```

Emergency mode can sustain 6.5 days on a 4,000 mAh battery.

---

## 5. Android Battery Optimization Interaction

Android's battery optimization features can suspend background processes:

### 5.1 Doze Mode

Android Doze (API 23+) restricts background apps when screen is off and device is stationary. Impact on IRIS:

- **Network access:** Blocked during Doze maintenance windows
- **Alarms:** `AlarmManager.setExact()` deferred; use `setAlarmClock()` for P0 heartbeats
- **Wake locks:** IRIS holds a partial wake lock during active relay

**IRIS mitigation:** Declare IRIS as a foreground service with ongoing notification. Foreground services are exempt from Doze restrictions on most Android versions.

```kotlin
// IRIS routing service — foreground service to avoid Doze
class IRISRoutingService : Service() {
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        startForeground(NOTIFICATION_ID, buildNotification())
        return START_STICKY
    }
}
```

### 5.2 Background Process Limits

Android O+ limits background services. IRIS uses:
- `JobScheduler` for non-urgent sync tasks
- `WorkManager` for periodic routing table aging
- Foreground service for the core relay loop (exempt from background limits)

### 5.3 Battery Saver Mode

When the user activates Android Battery Saver:
- IRIS reduces BLE scan duty to 1%
- LoRa RX duty reduced to 10%
- Wi-Fi Direct disabled
- P4+ relay rejected
- A persistent notification informs the user that IRIS is in reduced mode

---

## 6. iOS Background Fetch Limits

iOS restricts background execution more aggressively than Android:

| Mechanism | Max background time | IRIS use |
|---|---|---|
| Background App Refresh | 30 seconds per invocation | Routing table sync |
| PushKit / VoIP push | Unlimited while processing push | P0 wakeup (via APNs) |
| Background Bluetooth | Unlimited for connected peripherals | BLE relay |
| Location updates | Significant location changes | Mobility-triggered sync |

**Key constraint:** IRIS cannot maintain a persistent LoRa connection on iOS without hardware integration via External Accessory (MFi). For iOS, LoRa is available only via BLE-bridged external hardware.

**BLE on iOS background:** CoreBluetooth continues to receive peripheral advertisements and trigger callbacks in background. IRIS BLE relay remains functional when the app is backgrounded.

---

## 7. P0 SOS Battery Cost

The P0 SOS flow must not drain > 2% of battery for the complete send-and-acknowledge cycle:

```
P0 battery cost:
  Ed25519 sign:          < 1 mJ  (negligible)
  BLE/WiFi connection:   5 mJ   (100 mA × 50 ms)
  BLE transfer (256 B):  3 mJ   (30 mA × 10 ms × 10 hops)
  LoRa TX (256 B, SF9):  90 mJ  (120 mA × 750 ms)
  Epidemic flood (×20):  1800 mJ = 1.8 J

  3.7V × 4000 mAh = 14.8 Wh = 53,280 J total capacity
  2% = 1,066 J budget

  Epidemic flood with 20 copies via LoRa: 1.8 J ✓ (within 2% budget)
  Epidemic flood via BLE (all transports): ~0.5 J ✓
```

**P0 SOS is within battery budget** even for aggressive epidemic flooding.

---

## 8. References

- Android Doze optimization: https://developer.android.com/training/monitoring-device-state/doze-standby
- iOS background execution: https://developer.apple.com/documentation/backgroundtasks
- SX1262 datasheet: Semtech SX1262 data sheet, Table 10 (current consumption)
- BLE power: Nordic nRF52840 Product Specification, Section 6.3
- Performance budgets: `docs/performance/PERFORMANCE_BUDGETS.md`
