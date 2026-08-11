# Cellular Transport

## Overview

Cellular networks serve as a primary data transport in IRIS when available. Rather than building
cellular-specific protocols, IRIS treats cellular connectivity as an IP gateway — when a device
has cellular data, it behaves identically to any Internet-connected node. Cellular also provides
SMS as an emergency fallback channel and, on some networks, USSD as a last resort.

## India Coverage Landscape

### Operator Coverage (as of 2025)

| Operator | Technology | Coverage Area | Notes |
|----------|-----------|---------------|-------|
| Jio | 4G VoLTE, 5G (select cities) | ~99% of population, extensive rural | No 2G/3G fallback; voice is VoLTE-only |
| Airtel | 2G/3G/4G/5G | ~96% population coverage | Strong rural 2G fallback |
| Vi (Vodafone Idea) | 2G/3G/4G | ~80% population, urban focus | Financial instability; coverage shrinking |
| BSNL | 2G/3G/4G (partial) | Deep rural, government buildings | 4G rollout incomplete, slowest |

### Coverage Considerations for Disaster Response

During large-scale disasters, cellular infrastructure frequently fails first:
- Tower power fails within 4–8 hours without grid power (diesel generators extend to 48–72 hours)
- Backhaul fiber cuts disconnect towers even if towers have power
- Network congestion collapses usable throughput during mass emergencies
- Jio's VoLTE-only architecture means voice and data fail simultaneously

IRIS treats cellular as opportunistic — use it when available, do not depend on it.

### Signal Quality Detection

```rust
// Android via JNI bridge
// TelephonyManager.getSignalStrength() returns SignalStrength object
// Map to IRIS link quality score

pub enum CellularGeneration {
    G2,   // GPRS/EDGE — avoid for data, SMS may work
    G3,   // HSPA/HSPA+ — usable for P0-P3 only
    G4,   // LTE — full capability
    G5,   // NR — full capability, prefer for high-priority
    None, // No cellular
}

pub struct CellularState {
    pub generation: CellularGeneration,
    pub signal_dbm: i32,        // -50 (excellent) to -110 (marginal)
    pub operator: String,
    pub roaming: bool,
    pub data_connected: bool,
    pub sms_capable: bool,
}
```

## Android Telephony API

### Network Type Detection

```kotlin
// AndroidCellularTransport.kt
class AndroidCellularTransport(context: Context) {
    private val telephonyManager =
        context.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager

    fun getCellularGeneration(): CellularGeneration {
        return when (telephonyManager.dataNetworkType) {
            TelephonyManager.NETWORK_TYPE_GPRS,
            TelephonyManager.NETWORK_TYPE_EDGE,
            TelephonyManager.NETWORK_TYPE_CDMA,
            TelephonyManager.NETWORK_TYPE_1xRTT -> CellularGeneration.G2

            TelephonyManager.NETWORK_TYPE_UMTS,
            TelephonyManager.NETWORK_TYPE_EVDO_0,
            TelephonyManager.NETWORK_TYPE_EVDO_A,
            TelephonyManager.NETWORK_TYPE_HSDPA,
            TelephonyManager.NETWORK_TYPE_HSUPA,
            TelephonyManager.NETWORK_TYPE_HSPA,
            TelephonyManager.NETWORK_TYPE_HSPAP -> CellularGeneration.G3

            TelephonyManager.NETWORK_TYPE_LTE -> CellularGeneration.G4

            TelephonyManager.NETWORK_TYPE_NR -> CellularGeneration.G5

            else -> CellularGeneration.None
        }
    }

    // Requires READ_PHONE_STATE permission
    fun getSignalStrengthDbm(): Int {
        val ss = telephonyManager.signalStrength ?: return -120
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ss.cellSignalStrengths
              .maxOfOrNull { it.dbm } ?: -120
        } else {
            // Legacy: signal level 0-4 mapped approximately
            ss.level * (-20) - 40
        }
    }
}
```

### Signal Strength Thresholds

| dBm Range | Quality | IRIS Recommendation |
|-----------|---------|---------------------|
| > -70 | Excellent | All priorities allowed |
| -70 to -85 | Good | All priorities allowed |
| -85 to -95 | Fair | P0–P4 only |
| -95 to -105 | Poor | P0–P2 only, SMS fallback |
| < -105 | Marginal | SMS/USSD only |

## SMS as Emergency Fallback

### Android SMS

Android can send SMS without user interaction using `SmsManager`:

```kotlin
// IMPORTANT: Sending works without SMS_READ/RECEIVE permissions
// But receiving SMS programmatically requires RECEIVE_SMS permission
// (a sensitive permission, not granted by default)

class SmsEmergencyChannel(private val context: Context) {

    // Can send: YES, programmatically
    fun sendEmergencySms(destination: String, content: String) {
        require(content.length <= 160) { "SMS content exceeds 160 chars" }

        val smsManager = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            context.getSystemService(SmsManager::class.java)
        } else {
            @Suppress("DEPRECATION")
            SmsManager.getDefault()
        }

        smsManager.sendTextMessage(
            destination,      // IRIS node phone number or relay number
            null,             // service center — null = default
            content,
            null,             // sent PendingIntent
            null              // delivered PendingIntent
        )
    }

    // Can receive: ONLY with RECEIVE_SMS permission (sensitive, rarely granted)
    // Alternative: user opens SMS app, IRIS cannot auto-ingest without permission
    // Design decision: IRIS does not request RECEIVE_SMS in normal mode.
    // Emergency mode: user explicitly grants and enables SMS receive.
    fun registerSmsReceiver() {
        // Requires RECEIVE_SMS permission — request at emergency mode activation
        val filter = IntentFilter(Telephony.Sms.Intents.SMS_RECEIVED_ACTION)
        context.registerReceiver(smsReceiver, filter)
    }
}
```

### SMS Encoding for IRIS Messages

P0 SOS messages must fit in a single 160-character SMS (or 153 for multipart):

```
IRIS:P0:v1:<sender_id_12chars>:<lat_7>:<lon_7>:<msg_50>:<sig_8>
```

Example:
```
IRIS:P0:v1:A3B7F2C9D1E5:28.6139N:77.2090E:TRAPPED UNDER RUBBLE NEED RESCUE:A3B7F2C9
```

Total: ~90 characters. Leaves room for location precision.

### iOS SMS Limitations

**iOS cannot send SMS programmatically from an app.**

- `MFMessageComposeViewController` opens a pre-filled SMS compose sheet — user must tap Send
- No background SMS sending
- No SMS receiving API for apps
- This is a fundamental iOS platform restriction, not a missing permission

iOS emergency fallback: display the SMS pre-compose screen with a message pre-filled.
User must manually press Send. This is intentional — Apple prioritizes spam prevention.

## USSD as Last Resort

USSD (Unstructured Supplementary Service Data) is a GSM protocol that works even when:
- Data is unavailable
- SMS is congested
- Voice circuits are saturated

India emergency USSD codes are carrier-dependent. IRIS cannot initiate USSD sessions
programmatically on Android (dialer restriction). However, IRIS can prompt the user to
dial specific USSD codes to reach relay numbers.

```
*112#  — Emergency services (India)
*555#  — Carrier-specific emergency relay (proposed, not standardized)
```

## Cellular vs Wi-Fi Coexistence

Android and iOS both implement Wi-Fi/cellular coexistence at the OS level:

```kotlin
// Android: Force cellular even when Wi-Fi available (for IRIS relay traffic)
val connectivityManager =
    context.getSystemService(Context.CONNECTIVITY_SERVICE) as ConnectivityManager

val cellularRequest = NetworkRequest.Builder()
    .addTransportType(NetworkCapabilities.TRANSPORT_CELLULAR)
    .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
    .build()

connectivityManager.requestNetwork(cellularRequest, object : ConnectivityManager.NetworkCallback() {
    override fun onAvailable(network: Network) {
        // Bind IRIS relay socket to this network
        network.bindSocket(irisRelaySocket)
    }
})
```

IRIS strategy: use Wi-Fi for local mesh (BLE + Wi-Fi Direct), use cellular for Internet relay
traffic simultaneously. Android supports this via `bindSocket`; iOS does not expose this API.

## Cellular Data Offload

When IRIS relay traffic competes with user's normal data usage:
- Apply traffic classification: mark IRIS packets as background (low priority) by default
- Elevate P0–P1 traffic to foreground priority
- Android: `TrafficStats.setThreadStatsTag()` to mark threads
- Limit relay bandwidth to 10% of available throughput for P4+

## VoLTE Data Channel

VoLTE (Voice over LTE) uses an IMS data channel alongside voice. IRIS does not directly
use IMS, but VoLTE's existence means voice and data share the same LTE bearer — when
4G data is available, so is voice (no separate CS fallback needed on Jio).

## India-Specific Requirements

### SIM Requirements
- IRIS does not require any specific SIM configuration
- Works with any active SIM with data plan
- Emergency calling (112) works without SIM on all Indian networks (mandated by TRAI)
- No roaming restrictions for emergency 112 calls

### Emergency Number 112
- India's unified emergency number since 2018
- Replaces 100 (police), 101 (fire), 102 (ambulance)
- Works on any network, with or without SIM
- IRIS displays 112 prominently in P0 emergency flow alongside IRIS SOS

## Battery Impact

Cellular radio power consumption:

| State | Approximate Current | Notes |
|-------|-------------------|-------|
| Idle (registered, no data) | 10–20 mA | Tower ping every ~10 min |
| Data transfer (4G LTE) | 150–250 mA | Active transmission/reception |
| Data transfer (3G) | 200–300 mA | 3G has longer tail energy |
| Data transfer (5G NR) | 250–400 mA | Higher peak, shorter duration |
| SMS send | Brief 150 mA spike | <1 second |

**IRIS power budget**: cellular radio is the most power-hungry transport. IRIS activates
cellular only when:
1. Battery level > 30% (configurable threshold)
2. P0–P3 message needs relay
3. Cellular is the best/only path to destination

## Transport Capability Advertisement

```rust
pub struct CellularCapability {
    pub generation: CellularGeneration,
    pub signal_dbm: i32,
    pub operator: String,
    pub sms_available: bool,
    pub data_available: bool,
    pub battery_level: u8,  // 0–100
    pub willing_to_relay: bool,
}
```

Nodes advertise this in their capability bundle. Other nodes use it to route
Internet-bound messages through cellular-connected neighbors.
