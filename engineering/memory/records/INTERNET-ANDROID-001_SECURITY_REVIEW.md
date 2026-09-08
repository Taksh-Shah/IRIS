# INTERNET-ANDROID-001 — Security review

- **Date:** 2026-09-08
- **Verdict:** PASS WITH EXTERNAL GATES
- **Unresolved CRITICAL/HIGH findings:** none in the implemented software path

## Findings resolved

- Removed embedded production relay certificate/private key; server TLS
  material is runtime-provided and the client uses WebPKI trust.
- Bound relay registration to a fresh challenge and peer identity; enforced
  authenticated session source and target semantics.
- Split LAN and relay signature domains to prevent cross-protocol proof reuse.
- Added mutual Ed25519 LAN authentication; untrusted NSD metadata never creates
  trust or an active route.
- Scoped pooled sockets by authenticated peer/session identity and coalesced
  concurrent dials without sharing a stream across identities.
- Added bounded frames, queue bytes/items, active connections, inbound LAN
  sessions, timeouts, idle liveness, backoff, generation checks and persistent
  identity rate limits across reconnects.
- Prioritized registration acceptance before publishing a routable relay
  session, closing a pre-ACK race.
- Rejected production addressed plaintext at message ingress; Internet/LAN
  carries opaque end-to-end encrypted envelopes.
- Ensured callbacks do not block Android main/binder threads and stale NSD
  callbacks cannot withdraw a route still owned by another live service.
- Independent verification found a HIGH rapid stop/start race: stale successful
  listener cleanup could stop a newer generation. The native listener now has a
  worker-confined generation lease; stale cleanup may release only its own
  generation. Focused JVM regression and Kotlin compilation pass after the fix.

## Recorded limitations

- mDNS advertises the full stable PeerId on the local link. This is a privacy
  tradeoff for deterministic authenticated discovery and should move to rotating
  discovery aliases in a future protocol version.
- Real certificate issuance, relay host hardening/monitoring and public network
  exposure are deployment responsibilities and have not been evidenced here.
- Physical Android behavior remains AC-12/AC-13, not inferred from host tests.
