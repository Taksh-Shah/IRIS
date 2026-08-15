# CURRENT_MISSION.md

**Schema version**: 1.0
**Last updated**: 2026-08-11T19:30:00Z

---

## Mission Statement

Build the IRIS Resilient Communication Fabric — a multi-transport, disruption-tolerant mesh communication system that ensures no human being loses the ability to call for help during disasters.

## Current Mission Objective

Complete the PROTO-001 (Protocol Design) graph node: design and document the IRIS wire protocol including CBOR envelope format, addressing, versioning, and message types.

## Mission Context

The project has completed its documentation and planning phase. The engineering graph, policies, research, and architecture are all documented. We are now at the transition point where durable design gives way to implementation.

## What Success Looks Like

PROTO-001 reaches COMPLETE state when:
1. All acceptance criteria in PROJECT_GRAPH.yaml are satisfied
2. CBOR schemas defined for all message types
3. P0 SOS message confirmed to fit in 255-byte LoRa budget
4. Protocol specification documented in docs/protocol/
5. Size budget verified for all priority levels
6. ADR written for any new design decisions
7. Protocol test vectors written
8. Round-trip encode/decode property tests pass

## Higher-Level Mission Sequence

1. **COMPLETE**: Vision, requirements, architecture documentation
2. **ACTIVE**: Protocol design (PROTO-001)
3. **NEXT**: Cryptographic layer (CRYPTO-001) — requires human approval
4. **AFTER**: Transport abstraction, message engine, routing, store-carry-forward
5. **EVENTUAL**: Platform integration (Android/iOS/Desktop), pilot deployment

## Mission Constraints

- Never implement custom cryptography
- Never bypass regulatory constraints (LoRa duty cycle, spectrum)
- Never compromise P0 SOS delivery guarantees
- Never treat iOS and iOS as equivalent relay platforms
- Never claim forward secrecy that the construction doesn't provide
- Never fabricate evidence or maturity levels
