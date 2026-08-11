# LoRa Transport

## Overview

LoRa (Long Range) is a wireless modulation technology that enables long-range
communication at very low data rates. For IRIS, LoRa fills the critical gap between
short-range wireless (BLE/Wi-Fi, 10–100m) and satellite (very expensive, high latency).

**Range note:** Published LoRa range figures (e.g., "5–40 km") are experimental parameters
that vary significantly with antenna height, terrain, urban density, spreading factor,
interference, and regulatory power limits. These are NOT product guarantees. IRIS treats
range figures as simulation inputs requiring field validation (see `docs/simulation/LORA_SIMULATION.md`).
Indian urban terrain (Mumbai high-rises, dense Ahmedabad streets) will produce materially
different results than open rural terrain. All range claims must be labeled with their
evidence maturity level (RESEARCHED / SIMULATION_VALIDATED / FIELD_VALIDATED).

**Key constraint**: LoRa is not integrated into smartphones. IRIS uses external LoRa
hardware connected to a phone via BLE or USB. A phone becomes a LoRa gateway node.

## Physical Layer

### Chirp Spread Spectrum (CSS)

LoRa uses CSS modulation, not conventional radio modulation (FSK, OFDM):
- Signal is a chirp (frequency sweep from low to high or high to low)
- Chirp rate encodes data bits
- Spread across wide bandwidth → processing gain → can decode signals 20 dB below noise floor
- Enables reception even when signal is weaker than thermal noise

### Why CSS Matters for Disaster Comms

- **Interference immunity**: LoRa survives in high-interference environments (crowded
  disaster sites with many radio systems active)
- **Penetration**: sub-GHz frequencies penetrate buildings, vegetation, rubble
- **Non-line-of-sight**: links work through multiple walls and around terrain

## India Regulatory Framework

### WPC Spectrum Allocation

India's Wireless Planning and Coordination (WPC) wing under DoT has designated:

| Band | Frequency | Status | Max EIRP |
|------|-----------|--------|----------|
| SRD (Short Range Device) | 865–868 MHz (2021 Gazette) | License-exempt | 1W (30 dBm) |
| SRD | 915 MHz | NOT license-exempt in India | — |
| SRD | 433 MHz | License-exempt, lower power | 10 mW |

**⚠️ Frequency revalidation required:** The 2021 WPC Gazette updated the band to 865–868 MHz.
The older 865–867 MHz figure appears in pre-2021 documents and some secondary sources.
IRIS hardware and documentation must be validated against the current Gazette before production.
All IRIS LoRa hardware must be configured for this India-specific band (not 915 MHz, not 868 MHz EU).

### Duty Cycle Regulations

In the 865–867 MHz band:
- **Maximum duty cycle**: 1% per device
- Interpretation: for every 99 seconds of silence, 1 second of transmission allowed
- Equivalently: 36 seconds of transmission per hour maximum

This is not a soft guideline — violating duty cycle is a WPC violation. IRIS
enforces this in software.

### Equipment Certification

Commercial products operating in this band require:
- BIS (Bureau of Indian Standards) type approval
- WPC Equipment Type Approval (ETA)
- IRIS-branded hardware will require these certifications before sale
- DIY hobbyist use of uncertified hardware: legal grey area; WPC has not issued fines
  for small-scale experimentation at legal power levels

## LoRa Parameters

### Spreading Factor (SF)

SF determines the trade-off between range and data rate:

| SF | Data Rate (BW125) | Time on Air (255 bytes) | Range Factor |
|----|------------------|------------------------|--------------|
| SF7 | 5469 bps | ~0.56 s | 1x (baseline) |
| SF8 | 3125 bps | ~1.02 s | ~1.7x |
| SF9 | 1758 bps | ~1.82 s | ~2.7x |
| SF10 | 976 bps | ~3.28 s | ~4.2x |
| SF11 | 537 bps | ~5.9 s | ~6.4x |
| SF12 | 293 bps | ~10.5 s | ~9.4x |

IRIS default: **SF9** — balanced range (5–15 km typical) and duty cycle efficiency.
Emergency P0 messages: **SF12** — maximum range, burn the duty cycle.

### Bandwidth

| BW | Effect |
|----|--------|
| 125 kHz | Standard (best range/sensitivity) |
| 250 kHz | 2x faster, ~3 dB less sensitive |
| 500 kHz | 4x faster, ~6 dB less sensitive |

IRIS uses BW125 for all standard messages. BW500 reserved for gateway-to-gateway
high-speed bulk transfer when both nodes are in close proximity.

### Coding Rate

CR 4/5 (default) adds 25% overhead for error correction. CR 4/8 is only used if
link quality is below -15 dB SNR (monitored via RSSI/SNR feedback).

## Raw LoRa vs LoRaWAN

### LoRaWAN Architecture

```
End Device → Gateway → Network Server → Application Server → App
```

LoRaWAN requires gateway infrastructure (internet-connected base stations).
Used by TTN (The Things Network), Jio LoRaWAN (deployed in some Indian cities).

### IRIS Uses Raw LoRa

```
IRIS Node A → IRIS LoRa Gateway → IRIS Node B
```

Raw LoRa: direct point-to-point, no internet required, IRIS manages its own
addressing and routing. The IRIS protocol runs directly over LoRa physical frames.

### LoRaWAN Integration (When Available)

When a LoRaWAN gateway is reachable, IRIS can route messages via TTN or Jio LoRaWAN
as an alternative uplink path:

```rust
pub enum LoRaMode {
    Raw { frequency_hz: u32, sf: SpreadingFactor, bw: Bandwidth },
    LoRaWAN { app_eui: [u8; 8], dev_eui: [u8; 8], app_key: [u8; 16] },
}
```

LoRaWAN for IRIS: P0–P2 messages encoded in LoRaWAN payload, forwarded via
LoRaWAN network to internet, then to IRIS relay. Requires pre-provisioned
LoRaWAN credentials.

## Hardware

### Supported LoRa Hardware

| Device | Chip | Form Factor | Price (India) | Interface |
|--------|------|-------------|---------------|-----------|
| Heltec WiFi LoRa 32 v3 | SX1262 | Dev board (USB-C) | ~₹700 | USB Serial, BLE |
| TTGO LoRa32 v2.1 | SX1276 | Dev board | ~₹600 | USB Serial, BLE |
| RAK4631 WisBlock | SX1262 | Module | ~₹1,200 | USB, BLE |
| Seeduino WIO-E5 | STM32WLE5JC | Module | ~₹900 | UART |
| Custom IRIS hardware | SX1262 + ESP32-S3 | Rugged case | TBD | BLE only |

### Chip Comparison: SX1276 vs SX1262

| Feature | SX1276 | SX1262 |
|---------|--------|--------|
| Max power | +20 dBm | +22 dBm |
| Receive current | 11.5 mA | 6.4 mA |
| Transmit current (20 dBm) | 120 mA | 105 mA |
| Frequency accuracy | ±10 ppm | ±3 ppm |
| Support | Legacy | Current, preferred |

IRIS hardware specification: SX1262 required for new designs.

## Integration with IRIS Node (Phone as LoRa Gateway)

### BLE-Based Integration

```
User's Phone (IRIS app)
    ↓ BLE (GATT)
LoRa Gateway Device (ESP32 + SX1262)
    ↓ SPI
SX1262 LoRa Radio
    ↓ RF (865 MHz)
Remote LoRa Nodes
```

The ESP32 firmware presents a BLE GATT service with characteristics:
- `LORA_TX_CHAR (Write)`: phone writes LoRa frame to transmit
- `LORA_RX_CHAR (Notify)`: ESP32 notifies phone of received LoRa frame
- `LORA_STATUS_CHAR (Read/Notify)`: RSSI, SNR, duty cycle remaining, battery

```kotlin
// AndroidLoRaGatewayBridge.kt
class AndroidLoRaGatewayBridge(private val context: Context) : BluetoothGattCallback() {

    private var loRaTxChar: BluetoothGattCharacteristic? = null
    private var loRaRxChar: BluetoothGattCharacteristic? = null

    fun sendLoRaFrame(frame: LoRaFrame) {
        require(frame.payload.size <= 255) { "LoRa max payload 255 bytes" }
        loRaTxChar?.let { char ->
            char.value = frame.encode()
            gatt?.writeCharacteristic(char)
        }
    }

    override fun onCharacteristicChanged(
        gatt: BluetoothGatt,
        characteristic: BluetoothGattCharacteristic
    ) {
        if (characteristic.uuid == LORA_RX_UUID) {
            val frame = LoRaFrame.decode(characteristic.value)
            irisTransportLayer.receiveLoRaFrame(frame)
        }
    }
}
```

### USB-Based Integration

When BLE is unavailable or power-saving mode active, LoRa gateway connects via USB
serial (RNDIS or CDC ACM):

```rust
// Rust serial interface to LoRa gateway
use serialport::SerialPort;

pub struct UsbLoRaGateway {
    port: Box<dyn SerialPort>,
    duty_cycle_tracker: DutyCycleTracker,
}

impl UsbLoRaGateway {
    pub async fn transmit(&mut self, frame: &LoRaFrame) -> Result<()> {
        self.duty_cycle_tracker.check_and_consume(frame.time_on_air())?;
        let encoded = frame.encode_slip(); // SLIP framing for serial
        self.port.write_all(&encoded)?;
        Ok(())
    }
}
```

## Duty Cycle Enforcement

IRIS enforces the 1% duty cycle in software, even if hardware would allow more:

```rust
pub struct DutyCycleTracker {
    window_start: Instant,
    transmitted_ms_this_hour: u32,
}

impl DutyCycleTracker {
    const MAX_TX_MS_PER_HOUR: u32 = 36_000; // 1% of 3,600,000 ms

    pub fn check_and_consume(&mut self, time_on_air_ms: u32) -> Result<()> {
        self.advance_window();
        if self.transmitted_ms_this_hour + time_on_air_ms > Self::MAX_TX_MS_PER_HOUR {
            return Err(LoRaError::DutyCycleExceeded {
                available_ms: Self::MAX_TX_MS_PER_HOUR - self.transmitted_ms_this_hour,
            });
        }
        self.transmitted_ms_this_hour += time_on_air_ms;
        Ok(())
    }
}
```

**⚠️ NO PRIORITY OVERRIDE FOR DUTY CYCLE.** P0 SOS messages do NOT bypass duty cycle
enforcement. Regulatory duty cycle limits are hard legal constraints — not soft guidelines.
IRIS cannot declare an "emergency exception" to WPC regulations.

P0 handling within duty cycle limits:
- P0 messages are placed at the head of the transmission queue
- P0 transmits as soon as duty cycle budget allows (next available window)
- If duty cycle is exhausted, IRIS switches to alternative transports (BLE, Wi-Fi, Cellular, Satellite)
- Multi-transport simultaneous transmission maximizes P0 delivery probability within regulations
- A P0 SOS will NOT be held indefinitely waiting for duty cycle to reset; if no LoRa budget
  remains, the message is sent via the next-best available transport immediately

## Bandwidth Budget

Realistic throughput at SF9/BW125/1% duty cycle:

- Time on air per 255-byte packet: ~1.8 seconds
- Max transmissions per hour: 36,000 ms / 1,800 ms = 20 packets/hour
- Effective throughput: 20 × 255 bytes/hour = 5,100 bytes/hour ≈ **1.4 bytes/second**

This is extremely limited. IRIS LoRa message priority:
- P0 SOS: 60 bytes (critical fields only)
- P1 Evacuation: 80 bytes
- P2 Medical: 120 bytes
- P3 Logistics: 150 bytes
- P4+ never transmitted over LoRa

## Simulated LoRa Transport

For development and testing without hardware:

```rust
pub struct SimulatedLoRaTransport {
    pub sf: SpreadingFactor,
    pub bandwidth: Bandwidth,
    pub range_km: f32,
    pub packet_loss_rate: f32,
    pub duty_cycle_enforced: bool,
    pub propagation_delay_ms: u32,
}

impl TransportAdapter for SimulatedLoRaTransport {
    fn estimated_bandwidth_bps(&self) -> u64 {
        // SF9/BW125 = 1758 bps raw, ~1.4 bytes/second usable (duty cycle)
        14 // bytes per second
    }

    fn max_message_size_bytes(&self) -> usize { 255 }

    fn duty_cycle_remaining(&self) -> f32 {
        self.duty_cycle_tracker.remaining_fraction()
    }
}
```

The simulator injects configurable packet loss, propagation delay, and duty cycle
violations to test routing engine behavior under realistic LoRa conditions.
