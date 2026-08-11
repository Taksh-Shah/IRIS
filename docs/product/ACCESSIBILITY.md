# Accessibility Requirements

**Status:** Draft  
**Last updated:** 2026-08-11  
**Owner:** Product / Engineering  

---

## 1. Accessibility Standards and Targets

### 1.1 Compliance Targets

| Context | Standard | Level | Notes |
|---------|----------|-------|-------|
| Non-emergency UI | WCAG 2.1 | AA | All screens outside emergency flows |
| Emergency UI | WCAG 2.1 | AAA (enhanced) | SOS, authority broadcast, low battery mode |
| Screen reader support | Platform-native | Full | TalkBack (Android), VoiceOver (iOS) |

### 1.2 Rationale for Enhanced Emergency Accessibility

Standard WCAG 2.1 AA (4.5:1 contrast ratio, minimum 24px text) is insufficient for emergency use. In emergency conditions, users may:
- Have reduced vision from smoke, dust, or injury
- Be using their phone outdoors in bright sunlight (reduces effective display contrast)
- Have shaking hands (reduces fine motor precision)
- Have limited time to process visual information

IRIS emergency UI targets WCAG AAA: 7:1 contrast ratio, 24sp minimum text, oversized touch targets.

---

## 2. Visual Accessibility

### 2.1 Color Contrast

| UI State | Element | IRIS Target | WCAG Minimum |
|----------|---------|------------|-------------|
| Normal UI | Body text | 7:1 | 4.5:1 (AA) |
| Normal UI | Secondary text | 4.5:1 | 3:1 (AA large) |
| Emergency UI | All text | 7:1 | 7:1 (AAA) |
| Status indicators | Error/alert states | 7:1 | 3:1 (AA UI component) |
| Map UI | Text on map tiles | 4.5:1 | 4.5:1 (AA) |

Primary palette (accessibility-validated):

| Color | Hex | Usage | Contrast on white | Contrast on black |
|-------|-----|-------|------------------|------------------|
| IRIS Blue | #1A6FD4 | Primary actions | 5.8:1 | 3.6:1 |
| Emergency Red | #C0392B | Emergency UI background | — | — |
| Emergency Text | #FFFFFF | Emergency UI text on red | 7.2:1 vs #C0392B | — |
| Success Green | #1E7E34 | Delivered state | 5.1:1 | 4.1:1 |
| Warning Amber | #B45309 | Unverified alert | 5.4:1 | 3.9:1 |

Color must never be the sole means of conveying information. Every color-coded status also has:
- An icon (shape-differentiated)
- A text label

### 2.2 Text Size

| UI Context | Minimum Text Size | Scale Factor |
|-----------|------------------|-------------|
| Normal body | 16sp | Respects user font scale |
| Normal secondary | 14sp | Respects user font scale |
| Emergency body | 24sp | Fixed — does not scale down below 24sp even at small system scale |
| SOS button label | 32sp | Fixed |
| Authority broadcast title | 32sp | Fixed |

IRIS respects user system font scale for all non-emergency UI (Android: `sp` units; iOS: Dynamic Type). Emergency UI uses a minimum floor to prevent accessibility regression when system scale is set small.

### 2.3 Color-Independent Status Indicators

Mesh signal strength:

| State | Color | Icon | Text |
|-------|-------|------|------|
| No mesh | Red | ✗ (X in circle) | "No mesh" |
| Weak mesh | Orange | △ (triangle, 1 bar) | "Weak mesh" |
| Active mesh | Yellow | ◇ (diamond, 2 bars) | "Mesh active" |
| Strong mesh | Green | ✓ (checkmark, 3 bars) | "Strong mesh" |
| Satellite | Blue | ◉ (satellite icon) | "Satellite active" |

---

## 3. Screen Reader Support

### 3.1 TalkBack (Android)

**Global requirements:**
- All interactive elements have `contentDescription` that describes the action, not just the label
- Content descriptions use verbs: "Send SOS" not "SOS button"
- Status updates announced via `accessibilityLiveRegion` (ARIA-live equivalent)
- Custom views implement `AccessibilityNodeInfoCompat`

**SOS-specific requirements:**

```xml
<!-- Correct content description for SOS button -->
<Button
    android:contentDescription="Emergency SOS — double-tap to send your location to nearby devices"
    android:text="SOS" />
```

- SOS send confirmation: announced as: "Emergency SOS sent. Location: [location]. Time: [time]. Relay status: [N] devices have received your SOS."
- Relay count updates: announced via `ACCESSIBILITY_LIVE_REGION_POLITE` when relay count increases

**Map accessibility:**
- Map view: announces "Emergency map" on focus
- Other users' SOS locations: "Emergency alert from [Node ID] at [location], [time] ago"
- User's own location: "Your location: [coordinates], accuracy [N] meters"
- Map is not purely visual — all critical information accessible without seeing the map

### 3.2 VoiceOver (iOS)

**Minimum requirements:**
- All elements have `accessibilityLabel` and `accessibilityHint`
- Custom accessibility actions for complex gestures
- `UIAccessibilityAnnouncement` for real-time status updates

**SOS-specific:**
- Magic Tap (two-finger double-tap): configured as SOS shortcut for users who set this up in VoiceOver settings
- SOS confirmation: VoiceOver announces immediately, interrupting current focus
- Authority broadcast: `UIAccessibilityPostNotification(.announcement, ...)` with full broadcast text

**SwiftUI accessibility implementation:**

```swift
Button("SOS") { sendSOS() }
    .accessibilityLabel("Emergency SOS")
    .accessibilityHint("Sends your location to all nearby IRIS devices")
    .accessibilityAddTraits(.isButton)
```

---

## 4. Motor Accessibility

### 4.1 Touch Target Sizes

| Element | Minimum Size | IRIS Target |
|---------|-------------|------------|
| All interactive elements | 44×44 dp (WCAG 2.5.5) | 48×48 dp (Material Design) |
| SOS button | — | 80×80 dp minimum |
| Authority broadcast dismiss | — | 64×64 dp |
| Toggle switches | 44×44 dp | Full-width touch target |

### 4.2 SOS Gesture Alternatives

Standard SOS: 3 power button presses within 2 seconds. This may be impossible for users with:
- Tremor (Parkinson's disease, essential tremor): cannot time 3 presses accurately
- Limited strength: cannot press power button 3× rapidly
- Prosthetic hands: limited fine motor control

**Alternatives available:**
1. **Assistive Access mode (Android):** Simplified full-screen SOS button accessible via Accessibility shortcut
2. **Switch Access (Android):** SOS mapped to external switch device (bluetooth switch)
3. **Voice Access (Android / iOS):** Say "IRIS SOS" or "Send emergency" (requires voice access setup)
4. **SOS Lock Screen Widget (iOS 16+):** Large widget, single long-press to send SOS

**Settings path:** Settings > Accessibility > SOS Options > [Select alternative trigger]

### 4.3 No Time-Limited Interactions (Non-Emergency)

Non-emergency UI: no carousels, no auto-advancing slides, no timed dialogs. Exception: emergency P0 broadcast 30-second non-dismissal is a safety feature, not an accessibility barrier — the countdown is visible to screen reader users.

---

## 5. Language and Localization

### 5.1 Hindi and English

IRIS v1 (MVP) ships with full support for:
- **English (India English):** Primary development language; all strings available from day 1
- **Hindi (Devanagari):** Complete translation required before MVP launch

Hindi translation requirements:
- All UI strings translated (no English fallback for Hindi users)
- Text expansion: Hindi strings typically 20–40% longer than English; all UI must accommodate without truncation
- Right-to-left: Hindi is LTR; no RTL accommodation needed
- Numerals: Hindi uses Eastern Arabic numerals in some contexts; IRIS uses ASCII numerals for coordinates and technical data (universal); other numbers respect locale settings

### 5.2 Indian Regional Language Roadmap

| Language | Script | Target Users | Planned Milestone |
|----------|--------|-------------|------------------|
| Hindi | Devanagari | 600M+ | MVP |
| Bengali | Bengali | 100M+ | Alpha (Month 12) |
| Telugu | Telugu | 80M+ | Alpha (Month 12) |
| Tamil | Tamil | 75M+ | Beta (Month 18) |
| Marathi | Devanagari | 80M+ | Beta (Month 18) |
| Gujarati | Gujarati | 55M+ | GA (Month 30) |
| Kannada | Kannada | 60M+ | GA (Month 30) |
| Malayalam | Malayalam | 35M+ | GA (Month 30) |

Priority rationale: languages covering highest disaster-risk populations first (Odisha/Bengal floods, Andhra/Telangana cyclones, Tamil Nadu cyclones, Maharashtra/Gujarat earthquakes).

### 5.3 Emergency Language Considerations

SOS bundles and authority broadcasts include a language field (BCP 47 language tag). Receiving devices display broadcasts in the user's preferred language if a translation is available. If not, the original language is displayed with a note: "This message was not available in [user language]."

This requires authority broadcast senders to either: (a) send in multiple languages, or (b) accept that some users see the original language. NDRF and NDMA typically communicate in Hindi and English; these two languages cover the primary population for government deployments.

---

## 6. Testing and Validation

### 6.1 Automated Accessibility Testing

**Android:**
- Espresso with `AccessibilityChecks.enable()` in all UI tests
- `app:importantForAccessibility` audited via Lint rules
- Pre-commit hook: runs accessibility lint on changed layout files

**iOS:**
- Xcode Accessibility Inspector in CI
- SwiftUI `accessibilityElement` audit on changed views

### 6.2 Manual Testing Protocol

Before each release:
- [ ] All screens navigated with TalkBack enabled (Android)
- [ ] All screens navigated with VoiceOver enabled (iOS)
- [ ] SOS sent successfully with screen reader active
- [ ] Authority broadcast readable by screen reader
- [ ] All interactive elements reachable by keyboard (desktop Tauri app)
- [ ] Color contrast checked with Colour Contrast Analyser on exported screenshots

### 6.3 User Testing with Disabled Participants

Before Alpha launch: accessibility user study with:
- 3 participants with visual impairments (TalkBack/VoiceOver users)
- 2 participants with motor impairments (Switch Access users)
- Task: send SOS, receive authority broadcast, read a message

Pass criterion: all participants complete all tasks without assistance.

---

## 7. Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
