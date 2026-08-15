# Emergency UX Specification

**Status:** Draft — requires usability testing validation  
**Last updated:** 2026-08-11  
**Owner:** Product / Design  

---

## 1. SOS Flow

### 1.1 Trigger from Locked Screen

The primary SOS path requires no phone unlock and no app navigation.

**Android implementation:**
```
User action: Press power button 3× within 2 seconds
    ↓
Android system: triggers Emergency SOS (Android 12+ standard behavior)
    ↓
IRIS registered as emergency action: IRIS SOS activity launched
    ↓
IRIS SOS activity: full-screen red UI, no lock screen dismissal required
    ↓
Haptic: long-short-long pattern (500ms–250ms–500ms) × 3
Visual: red screen fades in (200ms transition), large "SOS SENDING" text
Audio: 1-second alert tone (WEA two-tone 853 Hz + 960 Hz, FCC WEA signal; C3)
    ↓
Bundle assembly: GPS fix attempt (5-second timeout) → send with best available location
    ↓
Bundle transmitted on all available transports simultaneously (BLE + Wi-Fi + LoRa)
    ↓
Confirmation: "SOS SENT — [location] — [timestamp]"
             "Relay status: [N devices received your SOS]"
```

**Timing targets:**
- From 3rd power press to SOS activity displayed: < 1 second
- From SOS activity displayed to bundle queued: < 2 seconds
- From bundle queued to first relay: < 5 seconds (within BLE range of peer)
- GPS fix acquisition: 5-second timeout (does not block SOS — sends with last known if GPS takes longer)

**iOS implementation:**
- iOS Emergency SOS (available from lock screen: hold side button + volume button) is system-controlled
- IRIS registers a SiriKit intent for "Send IRIS SOS" accessible via Siri shortcut
- From app: SOS button on main screen, always visible, single tap
- Lock screen widget (iOS 16+): IRIS SOS action available as lock screen widget

### 1.2 SOS Confirmation

After SOS bundle is queued for transmission, the confirmation screen shows:

```
┌──────────────────────────────────┐
│  SOS SENT                        │  ← Large, white text on red background
│                                  │
│  📍 Bandra West, Mumbai          │  ← Human-readable location from reverse geocode
│     18.9322°N, 72.8264°E         │  ← Raw coordinates (when geocode unavailable)
│     Accuracy: ±45 meters         │
│                                  │
│  ⏰ 14:32:07 IST                 │  ← Timestamp
│                                  │
│  📡 Relayed by 2 devices         │  ← Updated in real-time as relays confirmed
│                                  │
│  ─────────────────────────────── │
│  Add note (optional):            │
│  [Injured, trapped, 3rd floor  ] │  ← Pre-typed suggestions OR free text
│                                  │
│  [SEND UPDATE]  [CANCEL SOS]     │
└──────────────────────────────────┘
```

- "Relayed by N devices" counter updates in real-time as relay acknowledgments arrive
- Pre-typed note suggestions: "Injured", "Trapped", "Medical emergency", "Help others here", "Safe — just checking in"
- Cancel SOS: sends a "SOS cancelled" bundle to inform receivers; requires explicit confirmation

### 1.3 Resend and Update

After initial SOS:
- Auto-resend: initial P0 send, then duplicate every 30 s while unacked (2×),
  then unlimited until TTL expiry — aligned to ack.rs P0 policy (C5). No
  15-minute cadence.
- User can manually resend with updated location or note
- "SOS already sent" banner shown on main screen with last send time

---

## 2. SOS Confirmation: Haptic, Visual, and Audio

### 2.1 Haptic Pattern

| Event | Pattern | Duration |
|-------|---------|----------|
| SOS sent | Long-short-long (500ms-250ms-500ms) × 3 | ~3.75 seconds total |
| SOS received from another user | Short-short-long (200ms-200ms-500ms) × 2 | ~2.2 seconds |
| Authority P0 broadcast | Long × 5 (600ms each, 200ms gap) | ~4 seconds |
| Relay confirmed | Single short (100ms) × 1 | 0.1 seconds |

### 2.2 Visual Confirmation Sequence

```
0ms:    Red screen begins fade-in (from current app state or lock screen)
200ms:  Red fill complete; "SOS SENDING..." text appears
500ms:  Pulsing animation on "SOS" text (scale 1.0 → 1.05 → 1.0, 500ms period)
2000ms: "SOS SENDING..." → "SOS SENT" (text changes when bundle queued, not when delivered)
2200ms: Relay count appears ("Waiting for relay...")
Async:  Relay count updates as confirmations arrive ("Relayed by 1 device", "2 devices", ...)
```

### 2.3 Audio Alert

- Alert sound: distinct tone sequence (not a standard ringtone)
- Pattern: standard WEA two-tone 853 Hz + 960 Hz (FCC WEA alert tone; C3). Deliberate alternative patterns must be documented and are not recommended.
- Volume: tied to system volume; if device is silent/vibrate, use haptic only (do not override silent mode)
- Duration: plays once at SOS send; not looping
- Accessibility: described to screen reader users ("Alert: SOS has been sent")

---

## 3. Low Battery Emergency Mode UX

### 3.1 Trigger Conditions

Low battery emergency mode activates when:
- Battery level drops below 15%, OR
- User manually enables via Settings > Emergency Mode

### 3.2 UI Changes in Low Battery Mode

```
┌──────────────────────────────────┐
│  IRIS — LOW BATTERY              │  ← Simplified header (no menu, no navigation)
│  Battery: 12% — Emergency only  │
│                                  │
│  ┌────────────────────────┐      │
│  │       SOS              │      │  ← Giant SOS button, 80% of screen width
│  │   (tap to send)        │      │
│  └────────────────────────┘      │
│                                  │
│  📍 Location: ON  [toggle off]   │  ← Location toggle (off saves battery)
│  📡 Relay: ON  [toggle off]      │  ← Relay toggle
│                                  │
│  Messages and map disabled       │  ← Explanation text
│  to preserve battery             │
└──────────────────────────────────┘
```

### 3.3 Automatic Actions in Low Battery Mode

| Battery Level | Automatic Action |
|--------------|-----------------|
| < 15% | Enable low battery mode; notify user; pause map downloads |
| < 10% | Disable text messaging UI (relay continues for others' messages) |
| < 8% | Reduce BLE scan frequency from 1/s to 1/30s (saves ~40% drain) |
| < 5% | Disable Wi-Fi Direct (keep only BLE); disable LoRa (if active) |
| < 3% | SOS-only mode: BLE on for SOS transmission only; all other features suspended |

### 3.4 Battery Critical SOS

At < 3% battery, IRIS offers:
- One-tap SOS with current location
- After SOS, IRIS suspends all activity (app goes to background; single BLE advertisement every 5 minutes)
- Last SOS timestamp recorded in persistent storage (survives app kill)

---

## 4. Authority Broadcast Display

### 4.1 P0 (Critical) Authority Broadcast

Full-screen takeover for verified P0 authority broadcasts:

```
┌──────────────────────────────────┐
│ ■ OFFICIAL EMERGENCY ALERT       │  ← Red background; white text
│   VERIFIED — NDRF                │  ← Authority name and verification badge
│                                  │
│  EARTHQUAKE WARNING              │  ← Alert title (large: 32sp)
│                                  │
│  Evacuate Bandra East area.      │  ← Alert body (24sp)
│  Move to designated shelters     │
│  on Linking Road and             │
│  Turner Road.                    │
│                                  │
│  Do not use elevators.           │
│                                  │
│  ────────────────────────────── │
│  Issued: 14:28 IST               │
│  Valid until: 20:00 IST          │
│                                  │
│  [I UNDERSTAND — DISMISS]        │  ← Cannot be tapped for first 30 seconds
│  (Dismissable in 28 seconds)     │  ← Countdown timer
└──────────────────────────────────┘
```

- Full-screen takeover: IRIS comes to foreground regardless of current app or screen state
- Cannot be dismissed for 30 seconds (safety: user must see the complete message)
- If device is locked: shown on lock screen without requiring unlock
- Audio: distinct authority broadcast alert tone (different from SOS)
- Relay: P0 authority broadcasts are relayed by all nodes regardless of relay settings

### 4.2 P1 (Important) Authority Broadcast

Notification banner (does not interrupt current activity):

```
┌──────────────────────────────────┐
│ ■ NDRF: Road closures on NH-48   │  ← Banner notification, amber background
│   Verified official message ✓    │
│   [Tap to read full message]     │
└──────────────────────────────────┘
```

- Tapping opens the full broadcast view (not full-screen takeover)
- Persists in notification tray until dismissed by user
- Recorded in broadcast history (last 10 broadcasts accessible in app)

### 4.3 Unverified Broadcast

Any broadcast with an unknown or invalid signature:

```
┌──────────────────────────────────┐
│ ⚠ UNVERIFIED MESSAGE             │  ← Amber/yellow background
│   Source: Unknown                │  ← Cannot verify sender
│   "Evacuate immediately..."      │
│                                  │
│  This message has NOT been       │
│  verified by IRIS. It may be     │
│  misinformation. Check official  │
│  sources.                        │
│                                  │
│  [DISMISS]                       │
└──────────────────────────────────┘
```

---

## 5. Accessibility in Emergency

### 5.1 Screen Reader Support

**TalkBack (Android):**
- SOS button: content description "Emergency SOS button — double-tap to send SOS"
- SOS confirmation: announced immediately: "SOS sent — location: Bandra West Mumbai — time 2:32 PM"
- Authority broadcast: announced with alert tone followed by broadcast text read aloud
- Relay status updates: announced every 30 seconds if relay count changes

**VoiceOver (iOS):**
- Same content descriptions as TalkBack equivalents
- Magic Tap (two-finger double-tap): triggers SOS (VoiceOver accessibility shortcut)
- Authority broadcast: VoiceOver interrupts current announcement to read P0 broadcast

### 5.2 Large Text and High Contrast

Emergency UI minimum text sizes:
- SOS button label: 32sp (double the Material Design minimum)
- Alert message body: 24sp
- Status text: 18sp

High contrast requirements (emergency UI):
- Contrast ratio: minimum 7:1 (WCAG AAA) for all text
- No reliance on color alone to convey meaning (icons + text + color)
- Color blind safe: red/green status uses icon shape differentiation, not color alone

### 5.3 Motor Accessibility: SOS Alternative for Non-3-Press Users

Users who cannot perform 3 consecutive power presses (motor impairment):
- Settings: "SOS accessibility shortcut" → enables SOS via Assistive Access (Android) or Switch Access
- Lock screen: optional SOS tile (long-press accessible)
- Voice command: "Hey [assistant] — send IRIS SOS" via Android/iOS voice access (requires integration, Month 18)

---

## 6. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial specification |
