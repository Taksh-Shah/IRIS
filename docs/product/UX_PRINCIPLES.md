# UX Principles

**Status:** Approved  
**Last updated:** 2026-08-11  
**Owner:** Product / Design  

---

## Premise

IRIS is used by people in the worst moments of their lives. A person using IRIS may have just survived an earthquake. They may be injured. They may have lost family members. Their cognitive load is at maximum. Their fine motor control is impaired by stress.

Every UX decision must be evaluated against this context, not against the context of a person calmly setting up a productivity app.

The principles below are ordered by priority. When principles conflict, earlier ones take precedence.

---

## Principle 1: Works in 3 Taps Under Panic

**Definition:** The most critical action — sending SOS — must be reachable in 3 user actions or fewer from a locked screen, under simulated stress conditions.

**Rationale:** Research on stress and motor performance (Starcke & Brand 2012) shows that fine motor accuracy decreases significantly under acute stress. Complex navigation (menus, submenus, settings) fails under these conditions. The SOS path must require only gross motor actions — pressing a large button.

**Implementation requirements:**
- SOS gesture from locked screen: 3 consecutive power button presses (Android standard emergency gesture, requires no unlock)
- SOS confirmation: single large button tap; no text entry required for basic SOS
- SOS button: minimum 72dp × 72dp touch target (WCAG 2.5.5 Target Size)
- SOS button: always visible on the main screen (never behind a menu, never hidden)

**Testing approach:**
- Cognitive load test: participants perform a secondary task (counting backwards from 200) while attempting to send SOS
- Panic simulation: loud noise + time pressure while UI navigation is timed
- Pass criterion: 95% of participants send SOS within 10 seconds from locked screen

**Anti-patterns to avoid:**
- Confirmation dialog before SOS ("Are you sure?") — REMOVE
- Swipe gestures for SOS (require precision) — AVOID
- SOS behind a hamburger menu — NEVER
- SOS that requires unlocking the phone first — NEVER (must work from lock screen)

---

## Principle 2: No Account Required

**Definition:** IRIS must be fully functional from first launch without creating an account, entering an email, or completing registration.

**Rationale:** Account creation creates friction that will cause users to abandon onboarding in a disaster scenario. Requiring a server connection for activation means IRIS doesn't work when internet is down — which is exactly when it is needed.

**Implementation requirements:**
- First launch: node identity generated on-device (Ed25519 key pair) immediately, no server contact
- Display name: optional, local-only, not required for function
- Contact list: populated by meeting other IRIS nodes (no contact import, no phone book access required)
- Pairing: QR code scan or proximity BLE (no account-based pairing)
- Time to functional: < 30 seconds from app install to ready state

**Testing approach:**
- Naive user test: recruit participants with no prior IRIS exposure; measure time from install to first SOS sent
- Pass criterion: median time < 3 minutes; all participants functional within 10 minutes without assistance

**Anti-patterns to avoid:**
- Email verification
- Phone number verification (OTP SMS)
- Terms of service with mandatory checkbox (show on first use; no blocking behavior)
- "Activate your account" screen

---

## Principle 3: Works Offline from Day 0

**Definition:** Every core feature of IRIS must function without any internet or cellular connectivity, starting from the first time the app is opened.

**Rationale:** IRIS is a disaster communication tool. Disasters frequently take down internet and cellular infrastructure. An app that requires connectivity to activate, to download resources, or to sync is useless in the scenarios it is designed for.

**Implementation requirements:**
- App ships with: pre-bundled offline map tiles for 10 high-risk Indian districts (top-10 disaster-prone districts by historical frequency)
- App ships with: all routing algorithms, cryptographic libraries, and UI assets bundled (no CDN, no dynamic loading)
- Map tile download for additional districts: available over internet pre-disaster; graceful handling when download not possible
- Error states: no "network error" screens for core features; offline state is the default, expected state
- Startup: app fully functional within 5 seconds on a mid-range Android device without internet

**Testing approach:**
- Airplane mode test: enable airplane mode before first launch; verify all P0 features work
- No-WiFi-ever test: install on fresh device that has never connected to internet (sideload APK); verify function
- Pass criterion: SOS, location, text, map, and relay all functional in airplane mode from first launch

**Anti-patterns to avoid:**
- "Checking for updates..." blocking screen on launch
- Map tiles loaded from CDN (all tiles must be local)
- Routing tables downloaded from server
- Analytics or crash reporting that blocks UI if network unavailable

---

## Principle 4: Clear Signal Quality Indicator (Human-Readable, Not Technical)

**Definition:** IRIS must show the user a meaningful status of their mesh connectivity in plain language, without exposing technical implementation details.

**Rationale:** "PRoPHET delivery probability: 0.73" is meaningless to a disaster survivor. "Your SOS can reach 3 nearby people" is actionable. Users need to understand whether their device is connected and whether their messages will reach anyone — not the technical mechanism.

**Implementation requirements:**

Signal quality states (displayed in UI):

| Technical State | User-Facing Language | Color |
|----------------|---------------------|-------|
| No peers, no relay | "No mesh — you are alone" | Red |
| 1 peer within 2 hops | "Weak mesh — limited reach" | Orange |
| 2–5 peers within 3 hops | "Mesh active" | Yellow |
| >5 peers or gateway connected | "Strong mesh" | Green |
| Satellite relay active | "Satellite backup active" | Blue |

Additional context: "Your SOS has been relayed by 3 devices" (after SOS is sent)

Message delivery feedback:
- Sent: "Queued" (bundle in local store, not yet relayed)
- Relayed: "Relayed — waiting for delivery" (at least one hop forwarded)
- Delivered: "Delivered" (delivery confirmed by acknowledgment — when available)

**Anti-patterns to avoid:**
- PRoPHET scores in UI
- "3 relay hops" → say "Your message has been forwarded 3 times"
- Hop count raw numbers
- RSSI or SNR values in consumer UI (show only in developer/debug mode)

---

## Principle 5: Emergency UI is Distinct

**Definition:** The emergency state UI must be visually unmistakable from the normal state UI. A user must not need to think about whether they are in the emergency interface.

**Rationale:** Under stress, users default to pattern recognition. The SOS sent confirmation, the authority broadcast, and the incoming SOS alert must be visually distinct enough that their meaning is understood immediately, without reading.

**Implementation requirements:**

Emergency UI visual specification:
- Background: deep red (#CC0000 dark mode) or (#FF3B30 light mode)
- Text: white (#FFFFFF), minimum 24sp body text
- Icons: outlined, minimum 48dp
- Contrast ratio: minimum 7:1 (exceeds WCAG AAA requirement)
- Animation: no subtle animations; clear, immediate transitions
- Sound: distinct alert sound (different from all other app sounds)
- Haptic: distinct pattern (long-short-long, different from all other app haptics)

Scenarios requiring emergency UI:
- SOS sent confirmation screen
- Incoming SOS from another user
- P0 authority broadcast (full-screen takeover)
- Battery critical (< 10%) entering low-battery emergency mode

Non-emergency UI: normal app chrome with IRIS blue (#1A6FD4), relaxed visual design, standard text sizes.

**Anti-patterns to avoid:**
- Red used decoratively in normal UI (creates false emergency signals)
- Subtle color changes to indicate emergency (must be unmistakable)
- Same notification sound for all events

---

## UX Testing Approach for Disaster Scenarios

### Test Protocol 1: Stress Test (Lab)

1. Recruit 20 participants (mixed: tech-savvy and non-tech-savvy, age 20–60)
2. Assign to: low-stress (quiet room, no pressure) or moderate-stress (noise, secondary task, time pressure)
3. Task: "Send SOS; view a friend's location; receive an authority message"
4. Measure: task completion rate, time per task, error count, subjective difficulty rating
5. Pass threshold: 90% task completion under moderate-stress condition

### Test Protocol 2: Novice User Test (Field)

1. Recruit 10 participants with no prior IRIS exposure
2. Provide device with IRIS pre-installed, no instructions
3. Task: "Pretend there is an emergency. Do what you think you should do."
4. Observe without intervention
5. Measure: whether SOS was the first action, how long it took, what confused them

### Test Protocol 3: Accessibility Test

1. Participants: 5 users with visual impairments (using TalkBack/VoiceOver)
2. Task: send SOS and receive a message
3. Pass criterion: all core tasks completable with screen reader; SOS reachable within 30 seconds

---

## Revision History

| Date | Change |
|------|--------|
| 2026-08-11 | Initial document |
