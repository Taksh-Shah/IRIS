# PROTOCOL_CONFORMANCE fixtures (TEST-001 AC-7 / DEC-TEST-0007)

Fixture classes consumed by `../protocol_conformance.rs` (the committed harness).
The vectors are **embedded in the harness** so it is fully host-runnable, and are
hand-computed from the RFC texts (independent of the codec implementation) so a
buggy encoder cannot self-justify.

| Class | Vector | Provenance |
|-------|--------|------------|
| (a) RFC 8949 §4.2 CDE integer encodings | `0,1,23,24,255,256,65535,65536,1_723_334_400(u32::MAX+1-width)` | Hand-computed from RFC 8949 §3.3 Table 1 + §4.2 minimal-width rule |
| (a) RFC 8949 §3.1 definite-length byte strings | 256-byte P0 SOS payload => header `0x59 0x0100` | Hand pattern asserted at field 13 |
| (a) RFC 9562 field-2 UUIDv7 layout | version nibble `0x7`, variant bits `0b10` | MessageId round-trip conformance |
| (a) RFC 9171 §4.2.7 timestamp | Unix-epoch seconds as unsigned for `[0,1,1_700_000_000,1_723_334_400,u32::MAX]` | Known-epoch round-trip |
| (b) Bounded-recovery resync | tainted internet/TCP frame length bytes `[0xff,0xff,0xff,0x7f]`; beacon stream `[0xff x9][good beacon]` | Meshtastic pattern; resync must converge <= 512 B, no OOB read |
| (c) Capture-style PDU | u32-LE length-prefixed stream, sniffer dissect of envelope PDUs | Wireshark BP dissection pattern |
| (d) Property-based sim | `dense_mesh`/`partition_carry` seeds 0..8; `ForwardedCache` 64-id anti-loop | ns-3 BPv7 pattern over real Sim* coordinators |

Byte-exact wire vectors with full field tables live in
`tests/golden_vectors/` (AC-8). This directory is the provenance pointer for the
protocol-conformance classes so reviewers can trace every asserted byte to an
RFC text reference.