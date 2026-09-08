# ACTIVE NODE

**Schema version:** 1.0
**Last updated:** 2026-09-08T20:30:00+05:30

## ANDROID-UI-001 — Android UI P0 Remediation

- **Stage:** IN PROGRESS — commit `a67343c` lands WP1/WP6/WP7/WP8/P0#2/P0#3/P0#6
- **Software verdict:** source compiles; APK rebuild + device install needed for visual verification
- **Connected hardware:** `b2fbcd39`, vivo 2004, Android 12/API 31

### Completed in `a67343c`

| WP | What changed |
|---|---|
| WP1 + P0#2 | `InboxUiMessage.isOutbound: Boolean`; `sent()` → `isOutbound=true`; `pending()` → `isOutbound=true` |
| P0#3 | `DELIVERED` added to `DeliveryStatus`; `sent()` factory now uses DELIVERED immediately on engine accept |
| WP7 | `IrisMessage` right-aligns outbound, "You" label in AccentPrimary, delivery chips in header, reply suppressed for outbound |
| WP8 | Root `BoxWithConstraints` gains `imePadding()`; floating composer uses `navigationBarsPadding()` only |
| P0#6 | `POST_NOTIFICATIONS` removed from `MeshPermissions.required`; requested separately in `MainActivity` |
| WP6 | `IrisTheme.kt` wraps M3 `darkColorScheme` with IRIS palette; applied in `MainActivity.setContent` |

### Remaining open work (dependency-ordered)

| Order | Work package | Status |
|---|---|---|
| WP2 | Persistence (Room) — conversations/messages survive process death | ⬜ not started |
| WP3 | Runtime ownership — engine lifecycle to Application/Service scope | ⬜ not started |
| WP4 | Delivery bridge — correlate Rust ACK events with Kotlin message IDs | ⬜ not started |
| WP5 | IA + navigation — Chats → Conversation, Settings, Pairing routes | ⬜ not started |
| WP9 | Notifications — message channels, deep links, process-death receive path | ⬜ not started |
| WP10 | Pairing migration — move modal into navigation + theme | ⬜ not started |
| WP11 | Offline/network states — plain-language status copy | ⬜ not started |
| WP12 | Interactions — copy, reply, retry, message info, selection | ⬜ not started |
| WP13 | Verification — Compose tests, accessibility, screenshot baselines | ⬜ not started |

### Gate for INTERNET-ANDROID-001 (relay deployment)

No phone-reachable relay `IP:port` and no CA-certificate reference name are configured.
AC-12/AC-13 remain user-run physical verification.
Do not install an empty/localhost relay build.
