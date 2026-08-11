# IRIS System Boundaries

**Document ID:** IRIS-BOUND-001  
**Version:** 1.0

---

## 1. What IRIS Is

IRIS is a **multi-transport, heterogeneous, disruption-tolerant communication substrate** — not an end-user application.

| What IRIS Is | Elaboration |
|-------------|-------------|
| An offline-first mesh networking protocol | Operates without internet or cellular |
| A multi-transport abstraction layer | BLE, Wi-Fi Direct, Wi-Fi Aware, LoRa, Satellite, Internet |
| A store-carry-forward (DTN) engine | Messages persist through network partitions |
| An emergency priority system | P0–P7 with hard guarantees for P0 SOS |
| An end-to-end encrypted communication layer | Noise Protocol, Ed25519, X25519, ChaCha20-Poly1305 |
| A cryptographic identity system | Identity = Ed25519 key pair, no accounts |
| An edge node platform | Raspberry Pi class hardware for fixed relay |
| A gateway bridge system | Internet/LoRa/Satellite bridging |
| An open protocol | Documented, independently implementable |
| An SDK for application developers | Android (Kotlin), iOS (Swift), Desktop (TypeScript/Rust) |

---

## 2. What IRIS Is Not

| What IRIS Is NOT | Why |
|-----------------|-----|
| A social network | No profiles, followers, feeds, or social graph |
| A content platform | No hosting, sharing, or distribution of general content |
| A financial transaction system | No payment, wallet, or value transfer |
| An anonymous darknet | Privacy ≠ anonymity; compliance with lawful process |
| A replacement for emergency services | Complements 112/911; does not dispatch services |
| A centralized service | No IRIS-operated servers required for core operation |
| A surveillance tool | Cannot be used for mass monitoring without user consent |
| A VPN or censorship circumvention tool | Not designed for or marketed as such |
| A general-purpose internet proxy | Not a routing layer for internet traffic |
| A consumer messaging app | IRIS is infrastructure; apps are built on top |

---

## 3. Platform Boundaries

### 3.1 Mobile Platforms

| Platform | Minimum Version | Supported Transports | Notes |
|----------|----------------|---------------------|-------|
| Android | 8.0 (API 26) | BLE, Wi-Fi Direct, Wi-Fi Aware | Foreground service required for background mesh |
| iOS | 14.0 | BLE (central + peripheral), Wi-Fi (limited) | iOS restricts background BLE; emergency mode uses critical alert entitlement |
| Android Go | 8.1 (API 27) | BLE, Wi-Fi Direct | Optimized for 1GB RAM, constrained storage |
| Android TV | Not supported | N/A | No physical mobility; not a relay node |
| Wear OS | Future (v2.0) | BLE only | Limited storage; relay only, no origination |
| watchOS | Future (v2.0) | BLE only | Health emergency SOS via watch |

### 3.2 Desktop Platforms

| Platform | Implementation | Transports | Notes |
|----------|---------------|-----------|-------|
| Linux x86_64 | Tauri + Rust | BLE (BlueZ), Wi-Fi | Primary development platform |
| Linux ARM64 | Tauri + Rust | BLE (BlueZ), LoRa (via USB) | Raspberry Pi edge node |
| Windows 10+ | Tauri + Rust | BLE (WinRT), Wi-Fi | Desktop relay + gateway |
| macOS 11+ | Tauri + Rust | BLE (CoreBluetooth), Wi-Fi | Developer + edge use |

### 3.3 Edge Node Platform

| Hardware | OS | Role | Transports |
|----------|----|----|-----------|
| Raspberry Pi 4B | Raspberry Pi OS (Debian) | Full edge node + gateway | BLE, Wi-Fi, LoRa (USB/SPI), Ethernet |
| Raspberry Pi Zero 2W | Raspberry Pi OS Lite | Low-power relay | BLE, Wi-Fi |
| Orange Pi Zero | Ubuntu Server | Edge node | BLE, Wi-Fi, LoRa |
| GL.iNet travel router | OpenWrt | Wi-Fi mesh relay | Wi-Fi mesh |
| Custom PCB (future) | Zephyr RTOS | Constrained relay | BLE, LoRa |

### 3.4 Out-of-Platform Scope

- Feature phones (no Bluetooth 4.0+)
- 2G/GPRS-only devices
- Devices with Android < 8.0
- Smartwatches (v1.0 scope)
- IoT sensors (v1.0 scope; v2.0 may add constrained device support)
- Satellite handsets (IRIS runs on the device but does not replace the satellite handset software)

---

## 4. Network Boundaries

### 4.1 Transport In-Scope

| Transport | Frequency | Range | Throughput | Priority |
|-----------|-----------|-------|-----------|---------|
| BLE 5.x | 2.4 GHz ISM | 50–200m | 250 kbps–2 Mbps | Primary mobile transport |
| Wi-Fi Direct (P2P) | 2.4/5 GHz | 100–500m | Up to 250 Mbps | High-throughput local |
| Wi-Fi Aware (NAN) | 2.4 GHz | 50–250m | Up to 300 Mbps | Android-to-Android |
| LoRa 865–867 MHz | 865–867 MHz (India) | 2–30 km | 250 bps–5.5 kbps | Long-range low-rate |
| LoRa 915 MHz | 915 MHz (US/AU) | 2–30 km | 250 bps–5.5 kbps | Long-range (outside India) |
| Satellite (Iridium/GSAT/Starlink) | Ka/Ku/L band | Global | 64 kbps–100 Mbps | Emergency long-haul |
| Internet (TCP/IP) | Any | Global | Unlimited | When available |

### 4.2 Transport Out-of-Scope (v1.0)

| Transport | Reason |
|-----------|--------|
| HF Radio (HAM) | Complex hardware; licensed operators only |
| NFC | 5cm range; trivial coverage |
| Zigbee | IoT-focused; low adoption in smartphones |
| Z-Wave | Proprietary; home automation |
| TETRA/P25 | Government radio; separate integration needed |
| APRS | HAM amateur radio; separate integration |
| 4G LTE Direct (ProSe) | Limited device support; carrier-dependent |
| UWB (Ultra-Wideband) | Positioning only; <20cm data range |

### 4.3 Frequency Regulatory Scope

IRIS must comply with spectrum regulations in each deployment region:

| Region | LoRa Band | Regulatory Body | Notes |
|--------|-----------|----------------|-------|
| India | 865–867 MHz | TRAI/WPC | Unlicensed, max 1W ERP |
| EU | 868 MHz | ETSI EN 300 220 | Duty cycle limits apply |
| USA | 915 MHz ISM | FCC Part 15 | Higher power allowed |
| Australia | 915–928 MHz | ACMA | Similar to USA |
| Japan | 920 MHz | MIC | Japanese band plan |

IRIS must not transmit on frequencies that require licensing without proper licensing. The software must enforce regional frequency compliance.

---

## 5. Legal and Regulatory Scope

### 5.1 In-Scope Legal Considerations

| Area | Consideration |
|------|--------------|
| **Encryption** | End-to-end encryption compliant with Indian IT Act. No backdoors. IRIS does not facilitate or resist lawful interception orders differently from any other encrypted communication platform. |
| **Emergency services** | IRIS does not claim to provide 112/911 functionality. Users must be informed that IRIS SOS does not automatically contact emergency services. |
| **Data protection** | IRIS processes minimal personal data. No centralized storage. Compliant with DPDP Act (India) and GDPR principles. |
| **Radio licensing** | IRIS BLE and Wi-Fi transports operate in unlicensed ISM bands. LoRa operates in permitted unlicensed sub-GHz bands. Satellite gateway requires appropriate terminal licenses. |
| **Emergency broadcast** | Emergency broadcasts over IRIS are not official government alerts under the Disaster Management Act. IRIS must not be positioned as such. |

### 5.2 Out-of-Scope Legal Considerations

| Area | Why Out-of-Scope |
|------|-----------------|
| Content moderation | IRIS is end-to-end encrypted; relay nodes cannot read content |
| Copyright enforcement | Content policy is application layer responsibility |
| Anti-money laundering | No financial transactions |
| Know Your Customer (KYC) | No accounts; cryptographic identity only |
| PSAP integration | Not a registered emergency service |
| Lawful interception implementation | Complied with via standard legal process; no special backdoor |

---

## 6. Security Boundary

### 6.1 What IRIS Protects Against

| Threat | Protection |
|--------|-----------|
| Eavesdropping on messages | End-to-end encryption (ChaCha20-Poly1305) |
| Message forgery | Ed25519 digital signatures on all messages |
| Identity spoofing | Cryptographic identity; no impersonation without private key |
| Replay attacks | Message IDs + timestamps + nonce in encryption |
| Relay node snooping | Relay nodes handle ciphertext only; cannot read content |
| Network topology surveillance | Minimal metadata exposed in relay headers |
| Sybil attacks | Costly key generation; rate limiting on mesh participation |

### 6.2 What IRIS Does NOT Protect Against

| Threat | Why Outside Boundary |
|--------|---------------------|
| Device compromise | Compromised device = compromised keys. OS security boundary. |
| Traffic analysis at transport layer | BLE/Wi-Fi packet timing can be observed by RF-capable adversaries |
| Physical coercion to reveal keys | Social/legal boundary; beyond protocol scope |
| Nation-state RF monitoring | IRIS transmissions are detectable (radio emissions are observable) |
| Application layer vulnerabilities | Application security is the app developer's responsibility |
| Metadata at relay nodes | Source/destination addresses are visible to relay nodes in current design (v1.0 limitation; Sphinx routing considered for v2.0) |

### 6.3 Threat Model Scope

IRIS is designed to protect against:
- Passive eavesdroppers on wireless channels
- Malicious relay nodes (cannot read content, cannot forge messages)
- Network-level attackers who control some relay nodes

IRIS is NOT designed to protect against:
- Adversaries who control a majority of relay nodes (routing attacks possible)
- Adversaries with physical access to originating/receiving devices
- Adversaries capable of breaking Ed25519 or ChaCha20 (nation-state future adversary)

---

## 7. Data Boundaries

### 7.1 Data IRIS Collects (Minimized)

| Data | Where Stored | Who Can See | Retention |
|------|-------------|-------------|-----------|
| Ed25519 key pair | Local device only | Device owner | Until deleted |
| Message content | Local device + relaying devices | Sender + recipients (encrypted) | Per-message TTL |
| Routing metadata (message ID, priority, TTL) | Relay devices | Relay nodes (plaintext) | Until forwarded |
| GPS coordinates (optional, in SOS) | Encrypted in message payload | Recipients only | Per-message TTL |
| Network topology (neighbor list) | RAM only on each device | That device only | Not persisted |
| Message delivery log | Local device | Device owner | User-configurable |

### 7.2 Data IRIS Does NOT Collect

- No user registration data (no name, email, phone number)
- No persistent user profiles
- No advertising identifiers
- No behavioral analytics
- No centralized message logs
- No contact list access required (though optional for contact name resolution)
