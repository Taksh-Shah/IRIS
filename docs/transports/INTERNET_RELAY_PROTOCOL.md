# IRIS Internet Relay Protocol v1

Status: implementation contract for `iris-relay/1` (2026-09-08).

## Transport and trust

- TCP is protected by TLS 1.3 with WebPKI server authentication.
- The client requires ALPN `iris-relay/1`; the server terminates sessions that
  do not negotiate it.
- The relay sends a fresh random 32-byte challenge. The client registers its
  Ed25519 public key (the IRIS PeerId) and signs
  `"IRIS-RELAY-REGISTER-V1\0" || challenge || peer_id`.
- A registration proof is valid only for that TLS session. Route `source` must
  equal the authenticated registration identity.
- The first server `Heartbeat` after `Register` is the registration acceptance
  acknowledgement. The server queues it on the control channel before publishing
  the session, so no routed payload can overtake it.
- IRIS envelopes remain end-to-end encrypted. The relay can observe source,
  target, frame timing, and size; it is not a confidentiality endpoint.

## Framing

Every frame is exactly:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 1 | version (`1`) |
| 1 | 1 | kind |
| 2 | 32 | source / challenge |
| 34 | 32 | target |
| 66 | 4 | little-endian payload length |
| 70 | N | payload |

Kinds: `1 Register`, `2 Route`, `3 Heartbeat`, `4 Challenge`, `5 Ack`.
The maximum opaque route payload is 1 MiB. Empty routes, unknown versions,
unknown kinds, inconsistent lengths, zero identities, and invalid ACK statuses
are rejected.

`Route.payload = route_id[16] || iris_envelope`. `Ack.payload = route_id[16] ||
status[1]`; ACK target must match the route target. Status `1` means queued to a
currently live target session, not application delivery. Status `2` is target
offline and `3` is target backpressure. End-to-end delivery remains governed by
the message engine's signed/application ACK and store-carry-forward behavior.

## Liveness and resource policy

- Server heartbeat interval: 30 seconds; authenticated clients respond with a
  heartbeat. A client that sends no complete frame for 120 seconds expires.
- TLS, registration, read, and write operations have explicit deadlines.
- Server admission: 256 total sessions. The per-source-IP ceiling is configurable
  with `IRIS_RELAY_MAX_CONNECTIONS_PER_IP`, defaults to the global ceiling, and
  is clamped to it; this avoids rejecting many legitimate users behind CGNAT.
- Each authenticated identity: at most 120 route frames and 8 MiB of route
  payload per minute. This counter survives socket reconnects and the bounded
  identity table evicts expired windows before admitting new entries.
- Per-session output queue: 8 frames. Global queued-frame memory: 64 MiB,
  enforced by permits held until the writer consumes or drops each frame.
- Re-registering the same identity replaces the prior generation. Old-session
  cleanup cannot remove its replacement.

## Compatibility and tests

Changes to field meanings, signing domain, or framing require a new version or
ALPN. `relay_protocol.rs` contains byte-layout golden tests, round trips,
malformed-length tests, and optional `proptest` arbitrary-input coverage.
`relay_server.rs` tests challenge binding, generation safety, IP admission, and
queue/memory release, plus a real TLS 1.3/ALPN/authenticated bidirectional route
through the actual server accept loop. Internet transport integration tests exercise TLS relay,
authenticated LAN, IPv4/IPv6 dual stack, pooling, failover, and state loss.
