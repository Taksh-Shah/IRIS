# Engineering Changelog

All significant engineering changes, decisions, and milestones are recorded here.

Format: `## [version] - date - type - description`

Types: ARCHITECTURE | SECURITY | RESEARCH | DECISION | NODE | DOCUMENTATION | POLICY | BUG

---

## [0.2.0] - 2026-08-11 - ARCHITECTURE | SECURITY | POLICY

**Architecture Validation & Graph Integrity Pass — expert review findings addressed.**

### Critical Fixes

**Security — Cryptographic documentation corrected:**
- `ADR-0002`: Corrected Ed25519 SHA-512 claim (was incorrectly stated as "BLAKE3 in IRIS's case"; Ed25519 uses SHA-512 internally per RFC 8032; IRIS does not change this)
- `ADR-0006`: Added ⚠️ section documenting three open architecture issues requiring resolution BEFORE implementation:
  1. Per-session vs per-message ephemeral key inconsistency (ADR says "per session"; MESSAGE_ENVELOPE shows per-message header — materially different)
  2. "HKDF-BLAKE3" is a non-standard construction — requires decision (HKDF-SHA256 or BLAKE3 native KDF)
  3. Current ECIES-like construction does NOT provide forward secrecy against recipient key compromise — must be accurately documented; do not claim forward secrecy without qualifying it
- **IMPLEMENTATION GATE: CRYPTO-001 remains BLOCKED pending human cryptographic architecture review resolving the above**

**Regulatory — LoRa duty cycle override removed:**
- `docs/transports/LORA.md`: Removed "P0 SOS messages ignore duty cycle tracking" — this was a regulatory violation. P0 messages are prioritized in queue but MUST NOT bypass WPC duty cycle limits. Alternative transports used when LoRa budget exhausted.

**Regulatory — India spectrum frequency corrected:**
- `docs/transports/LORA.md` + `docs/legal/INDIA_COMPLIANCE.md`: Flagged 865–867 MHz → 865–868 MHz per 2021 WPC Gazette. All spectrum claims require validation against current Gazette before production.

**Legal — Fabricated organizational fact removed:**
- `docs/legal/INDIA_COMPLIANCE.md`: Removed "IRIS has engaged qualified Indian telecommunications legal counsel" — this was a fabricated fact. Replaced with: "Legal counsel review is required before any commercial or government deployment."

**Legal — DPDP Rules 2025 added:**
- `docs/legal/INDIA_COMPLIANCE.md`: Added Section 2A documenting the Digital Personal Data Protection Rules 2025 (notified by MeitY, November 2025). All legal compliance work must address both the Act 2023 and the Rules 2025.

### Architecture Improvements

**Graph integrity — Missing edges added:**
- `engineering/PROJECT_GRAPH.yaml`: Added 18 missing edges that were declared in node dependencies but absent from the edges section. Affected: WIFIDIRECT-001, SEC-001, SIM-001, ANDROID-001, IOS-001, DESKTOP-001, OBS-001, TEST-001, LEGAL-001.
- Graph version bumped to 0.2.0.

**Project state — Corrected to match graph:**
- `engineering/PROJECT_STATE.yaml`: Corrected state counts to match actual graph (was: 1 complete, 3 in_progress, 28 discovered; now: 1 complete, 3 research_complete, 4 designing, 24 discovered). Added note that this file must be generated from graph, not maintained independently.

**Platform capability model — Promoted to first-class:**
- `docs/architecture/PLATFORM_CAPABILITY_MODEL.md`: New document establishing iOS/Android background execution constraints as architecture facts, not footnotes. Defines four relay capability tiers. Architecture must never treat iOS as equivalent Android relay.

### Policy Improvements

**Evidence maturity model added:**
- `engineering/RESEARCH_POLICY.yaml`: Added 11-level evidence maturity model (HYPOTHESIZED → CERTIFIED). Every technical claim must be tagged. Defines what level of evidence is required for different claim types (regulatory claims require CERTIFIED, performance benchmarks require HARDWARE_VALIDATED, range claims require HARDWARE_VALIDATED, etc.).

**Formal invariants defined:**
- `engineering/FORMAL_INVARIANTS.yaml`: New file. Defines 12 system invariants across security, routing, deduplication, storage, privacy, and emergency categories. Each invariant has a test type, test ID, and fuzz target requirement.

**New agents added:**
- `engineering/AGENT_REGISTRY.yaml`: Added Independent_Verification_Agent (verifies node correctness independently of builder) and Red_Team_Architect (adversarially stress-tests the architecture to find failure modes before implementation).

### Known Open Issues (not yet resolved — require follow-up)

1. **Graph size**: 32 nodes insufficient for full system coverage; need 50+ additional nodes for protocol conformance, platform details, security properties, CI/CD, supply chain, product features
2. **Crypto architecture**: per-session vs per-message ephemeral key unresolved; HKDF-BLAKE3 non-standard; forward secrecy properties need clarification
3. **Dependency model**: typed dependencies ({node, relationship, required_state}) not yet implemented in node schema
4. **Supply chain security**: no graph branch for SBOM, dependency pinning, artifact signing
5. **Protocol conformance testing**: no PROTOCOL_CONFORMANCE node
6. **Competitive analysis**: needs primary-source confidence levels (Documented/Observed/Reported/Inferred/Unknown)
7. **Reproducibility**: benchmark documentation needs hardware/OS/seed fields mandated in CI

---

## [0.1.0] - 2025-01-01 - DOCUMENTATION

**Initial documentation and engineering control plane created.**

### Added
- Full documentation tree (`/docs`) covering all subsystems
- Root project documents: charter, vision, problem definition, use cases, system boundaries, glossary
- Architecture documentation: system architecture, reference architecture, layer model, node model, network graph, temporal graph, data flow, control flow, failure architecture, gateway architecture, edge architecture, architectural principles
- Protocol documentation: overview, message model, addressing, message envelope, delivery policies, priority model, TTL, ACKs, retries, deduplication, fragmentation, reassembly, synchronization, versioning, compatibility
- Transport documentation: abstraction, BLE, Bluetooth Mesh, Wi-Fi Direct, Wi-Fi Aware, Wi-Fi, Ethernet, Cellular, Internet, LoRa, Satellite, USB, Future Transports
- Routing documentation: architecture, requirements, baseline, multipath, opportunistic, DTN, store-carry-forward, gateway selection, network healing, congestion control, routing security, experiments
- Identity documentation: architecture, cryptographic identity, key management, trust model, authorization, revocation, verified entities, privacy model
- Security documentation: architecture, threat model, attack surface, adversary model, Sybil resistance, replay protection, message authentication, routing attacks, DoS resistance, spam resistance, emergency abuse, privacy threats, security testing
- Safety documentation: safety charter, responsible use, abuse prevention, emergency governance, authority verification, public channel moderation, incident response, misinformation, law enforcement requests, safety risk register
- Emergency documentation: architecture, SOS, emergency broadcast, priority delivery, location sharing, disaster modes, crowd management, search and rescue, emergency scenarios
- Intelligence documentation: architecture, deterministic layer, graph intelligence, statistical layer, ML layer, RL layer, LLM layer, model evaluation, AI safety
- Simulation documentation: architecture, network simulator, mobility models, failure models, traffic models, satellite simulation, LoRa simulation, scale testing, simulation validation
- Platform documentation: architecture, Android, iOS, Windows, macOS, Linux, cross-platform
- Implementation documentation: repository architecture, Rust core, Kotlin layer, Swift layer, TypeScript layer, Python layer, storage, observability, configuration
- Testing documentation: strategy, unit testing, integration, property testing, fuzzing, adversarial, interoperability, cross-platform, failure testing, regression
- Performance documentation: performance model, benchmarking, battery, bandwidth, latency, scale, performance budgets
- Operations documentation: observability, telemetry, logging, diagnostics, deployment, field operations, incident management
- Research documentation: methodology, technology landscape, competitive analysis, academic research, standards review, research gaps, open problems
- Legal documentation: legal research, India compliance, privacy requirements, telecom considerations, spectrum, satellite regulation, data governance, compliance risk register
- Product documentation: requirements, UX principles, emergency UX, accessibility, product roadmap
- Business documentation: business model, B2B, B2G, enterprise, SDK, hardware, go-to-market
- Decisions: ADR index, ADR-0001 through ADR-0006
- Requirements: REQUIREMENTS_INDEX.md with REQ-001 through REQ-020
- Experiments: EXPERIMENT_INDEX.md with EXP-001 through EXP-005
- Benchmarks: BENCHMARK_INDEX.md with BENCH-001 through BENCH-005

### Engineering Control Plane (`/engineering`)
- PROJECT_GRAPH.yaml: 32-node dependency graph with typed edges
- NODE_SCHEMA.yaml: complete node schema definition
- EDGE_SCHEMA.yaml: edge type definitions
- AGENT_REGISTRY.yaml: 20 specialized agents with missions, capabilities, constraints
- AGENT_CAPABILITIES.yaml: capability-to-agent mapping
- EXECUTION_POLICY.yaml: autonomy level 4, allowed/forbidden actions, approval gates
- RISK_POLICY.yaml: risk levels and thresholds
- APPROVAL_POLICY.yaml: high-risk gate definitions
- ACCEPTANCE_POLICY.yaml: node completion criteria
- RESEARCH_POLICY.yaml: evidence hierarchy and research methodology
- TEST_POLICY.yaml: testing requirements and coverage targets
- SECURITY_POLICY.yaml: security engineering requirements
- DOCUMENTATION_POLICY.yaml: documentation standards
- PRIORITY_POLICY.yaml: priority calculation formula
- LOOP_POLICY.yaml: node execution loop definition
- ESCALATION_POLICY.yaml: when and how to escalate
- AUTONOMY_POLICY.yaml: autonomy levels and boundaries
- PROJECT_STATE.yaml: current project state snapshot
- CHANGELOG.md: this file

### Key Decisions (in ADRs)
- ADR-0001: Rust as primary networking core (ACCEPTED)
- ADR-0002: CBOR as wire format (ACCEPTED)
- ADR-0003: Ed25519 + X25519 + ChaCha20-Poly1305 cryptographic suite (ACCEPTED, pending crypto review)
- ADR-0004: BLE as primary short-range discovery transport (ACCEPTED)
- ADR-0005: Monorepo architecture (ACCEPTED)
- ADR-0006: Opportunistic routing with PRoPHET + spray-and-wait (PROVISIONAL)

### Initial Project Graph
- 32 engineering nodes defined
- Critical path identified: VISION → REQ → ARCH → PROTO → MSG → ROUTE → SCF → EMERG → ANDROID → PILOT
- 5 high-risk nodes requiring human approval identified
- 5 research gaps identified with planned experiments
- 5 open legal questions requiring lawyer review

### Impact
- Autonomous engineering system has complete blueprint to begin implementation
- All policies defined for autonomous operation
- High-risk gates established
- Next recommended action: begin PROTO-001 (Protocol Design implementation)
