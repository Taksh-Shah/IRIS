# Android Internet and LAN Transport Design

**Status:** Design gate — implementation not yet started  
**Date:** 2026-09-07  
**Scope:** Tier 5, HV-42..45

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
The peer identity is not the mDNS name or IP; the first authenticated IRIS
envelope/handshake establishes identity. Discovery callbacks are serialized,
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

- no direct inbound listener on the phone;
- no STUN-only promise;
- no bypass of IRIS envelope verification;
- no claim that a relay is available until the service endpoint and protocol are
  implemented and independently verified.

Research basis: `engineering/memory/records/research/RES-0031.md`.
