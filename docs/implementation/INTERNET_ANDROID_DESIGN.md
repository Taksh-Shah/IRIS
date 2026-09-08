# Android Internet and LAN Transport Design

**Status:** Recovery design v2 — accepted for implementation
**Date:** 2026-09-08
**Scope:** INTERNET-ANDROID-001, Tier 5, HV-42..45, FAIL-0009

## Goal

Give the Android engine two usable IP paths while preserving the existing IRIS
transport abstraction and envelope security:

1. same-LAN discovery and delivery using NSD/DNS-SD plus TCP;
2. cross-network delivery through an authenticated outbound relay.

The relay path is the internet baseline. Direct NAT traversal is optional future
work and cannot be an acceptance prerequisite.

## Proposed seams

### Android network signal

`IrisApplication`/the foreground service owns one lifecycle-bound
`ConnectivityManager.NetworkCallback`. It reports a small immutable snapshot:
`online`, `validated`, `metered`, and the active network identity. Registration and
unregistration are idempotent. A callback never performs blocking I/O.

### LAN discovery

The Android adapter registers `_iris._tcp` with a versioned TXT payload containing
only bounded, non-secret candidate metadata. `NsdManager` discovery resolves the
service to an address and port, then emits a `PeerInfo` with an `internet` address.
The peer identity is not the IP address. The TXT PeerId is an untrusted routing
hint; the mutual Ed25519 handshake proves that the endpoint owns it before any
frame is accepted. Discovery callbacks are serialized,
cancelled on shutdown, and stale services expire from the neighbour table.

### Internet relay

The Rust `InternetTransport` remains responsible for framing, pooling, retry
backoff, and per-peer binding. Android supplies relay configuration and network
state; it does not create a second transport implementation in Kotlin. The
relay protocol must provide authenticated TLS, bounded frames, peer/session
binding, liveness, and explicit failure states. The current plaintext core path is
therefore not production-complete and must not be marked hardware-accepted merely
because a socket connects.

### Selection and lifecycle

The manager may select LAN TCP when a resolved local endpoint exists and the
network is unmetered/preferred. It may select relay TCP when configured and the
network is validated. Loss of the active network causes `Degraded`/`Unavailable`
and bounded reconnect, never a permanent `Connecting` state. Existing BLE,
Wi-Fi Direct, and Wi-Fi Aware registrations remain unchanged.

## Acceptance plan

- **HV-42:** Android engine registers the IP transport; unit tests prove framing,
  binding, lifecycle, and selection; two-phone LAN and relay tests prove delivery.
- **HV-43:** two phones on one infrastructure Wi-Fi discover one another via
  `_iris._tcp`, exchange authenticated messages, and recover after NSD restart.
- **HV-44:** server deployment is required; two phones on different networks
  exchange messages through the relay, including reconnect/offline behavior.
- **HV-45:** callback transitions are unit-tested and visible in `/diag`; hardware
  toggles Wi-Fi/mobile connectivity and confirms deterministic state changes.

Every physical run must follow the hardware loop: fresh APK, controlled topology,
logcat plus mesh snapshots, repeated passes, failure-focused re-research, and a
durable evidence record before closure.

## Explicit non-goals

- no public-WAN inbound listener on the phone (a bounded same-LAN listener is
  required for symmetric local messaging);
- no STUN-only promise;
- no bypass of IRIS envelope verification;
- no claim that a relay is available until the service endpoint and protocol are
  implemented and independently verified.

Research basis: `engineering/memory/records/research/RES-0031.md` and
`engineering/memory/records/research/RES-0034.md`.

## Recovery architecture (v2)

### Configuration and truthful state

Relay configuration is one atomic tuple: one or more numeric socket endpoints,
the certificate DNS/IP reference identity, and this node's Ed25519 PeerId. An
empty tuple is valid but produces `relay_not_configured`; it must never be
reported as a radio failure. Android reports `validatedWan` and `localWifi`
separately. The Rust transport is selectable when either a LAN listener is live
on local Wi-Fi or a relay is configured on validated WAN. Diagnostics expose the
four inputs and the last failure reason.

`start_all` tries every radio, then regards `internet-0` as started when its state
is Available/Connected/Degraded. NSD starts before this check so same-LAN mode can
bring the mesh up even on infrastructure Wi-Fi without public Internet.

### Trust and discovery boundary

DNS-SD TXT data is candidate metadata, never a trust assertion. Android accepts
only `v=1`, a canonical 64-hex non-self PeerId, a valid port, and a numeric
address. Rust publishes a LAN candidate to `DiscoveryManager` only if that
PeerId is already `Verified`/`AuthorityRoot`; legacy bare-key presence is not
sufficient for Internet activation. A hint discovered before confirmation is cached and retried
when confirmation succeeds. `onServiceLost` removes the exact service-to-peer mapping and
marks the Internet link down. IPv6 literals use bracketed socket syntax.

The same explicit trusted-peer API creates a relay `PeerInfo` with the configured
relay endpoint and calls `connect`; pairing never learns transport addresses, and
transport discovery never creates trust. Startup, successful verification, and
network recovery re-run this idempotent bootstrap.

### Path selection and recovery

For a known peer, `send` tries the current LAN mapping first. A connection/write
failure evicts that exact mapping and immediately falls back to the peer's relay
binding when validated WAN is available. Relay candidates have independent
failure counters and retry deadlines; success is recorded only after TCP, TLS,
ALPN, challenge registration, and relay acknowledgement. Pool idle expiry is
shorter than either side's read timeout. Pool keys include socket address and
authenticated scope (remote identity for LAN, local registration identity for
relay); per-key dial locks and shared writers coalesce concurrent activations.
Listener shutdown clears the published port, peers, accept task, and every
bounded inbound session; restart always binds a fresh live socket.

### Relay authentication and delivery contract

The server sends a random 32-byte challenge after TLS. The client signs the
domain-separated tuple `(protocol version, challenge, PeerId)` with the Android
Keystore Ed25519 identity key. The server verifies strictly using PeerId as the
public key, caches challenges for one connection only, then binds that session.
Every Route source must equal the bound identity. Registration replacement uses
a generation token so an old disconnect cannot erase a newer session.

Route frames carry the 16-byte message id. The server returns a bounded status
ACK to the source: `forwarded_to_live_session`, `target_offline`, or
`target_backpressured`. The client returns `SendReceipt` only for the first
status; the status does not claim application-level delivery, which remains the
message engine's signed/end-to-end ACK responsibility.

The router holds no global mutex across I/O. Each registered session owns a
bounded writer channel. A semaphore caps concurrent connections; TLS handshake,
registration, reads, writes, and ACK waits are timed; malformed/source-spoofed
frames close the session. TLS is restricted to 1.3 and both sides require exact
ALPN `iris-relay/1`.

### Server deployment

`iris-relay-server` requires bind address, certificate chain, and private-key
paths (arguments or documented environment variables). It has no embedded
production certificate or key. The self-signed localhost material lives only in
`#[cfg(test)]` fixtures. A release process supplies a CA-valid certificate whose
SAN matches Android's configured server name.

### Android lifecycle

`NetworkCallback.onCapabilitiesChanged` consumes the callback's supplied
capabilities; `onAvailable` waits for that event instead of doing a racy
synchronous lookup. Loss is network-identity-aware. NSD holds a non-reference-
counted multicast lock only while active, serializes lifecycle and legacy
resolution on the main looper, token-checks stale resolve callbacks, and releases
every resource on stop/failure. Connectivity and NSD callbacks never call Rust
directly; a background dispatcher/single worker performs FFI operations.

## Implementation order

1. Relay codec authentication/challenge/ACK types and adversarial tests.
2. TLS 1.3 + exact ALPN helpers; runtime certificate/key server configuration.
3. InternetTransport state split, relay bootstrap/ACK, LAN fallback, restart and
   backoff repairs.
4. Android FFI configuration/diagnostics/trusted-peer lifecycle and engine status.
5. Android callback, NSD/multicast/IPv6/loss lifecycle, build configuration.
6. Focused → workspace → JVM → ABI/APK → physical LAN → deployed WAN tests.

## Acceptance boundary

Software may advance through SECURITY_REVIEW and VERIFY with AC-12/AC-13 marked
resource-gated. The node cannot become COMPLETE until two-phone same-LAN and
different-network tests provide durable logs against the exact APK and a named,
CA-valid deployed relay. A green loopback/self-signed integration test is not a
substitute for either physical criterion.
