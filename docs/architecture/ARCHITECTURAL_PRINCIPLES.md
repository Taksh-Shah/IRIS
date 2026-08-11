# Architectural Principles

These are non-negotiable engineering principles. Every design decision is evaluated against them. Violations require explicit justification, documented in an ADR, and reviewed by the team.

These principles exist because IRIS will be used in the worst moments — disasters, emergencies, infrastructure collapse. Cutting corners here has life-safety consequences.

---

## 1. Never Assume Internet Availability

Every feature must function without Internet. Internet is an optimization, not a requirement.

**Rationale:** The scenarios where IRIS matters most (earthquakes, floods, cyclones, infrastructure attacks) are exactly the scenarios where Internet infrastructure fails first. A system that degrades to nothing without Internet is useless in a disaster.

**Implication:** No feature may be marked "online only" except analytics/telemetry. Every UI state has an offline-capable path. Every routing algorithm functions without any external server. The mesh must be complete in itself.

**Violation signal:** "This feature requires the server to..." → redesign.

---

## 2. Never Assume Cellular Availability

Cellular can fail during disasters. Cellular is one transport among many.

**Rationale:** Cell towers lose power, get overloaded, sustain physical damage. During the 2001 Bhuj earthquake, all cellular communication was down for 24-72 hours. During Chennai floods (2015), cellular networks were congested to unusability.

**Implication:** Offline mesh must be complete without cellular. Cellular is treated as a bonus Internet gateway, not as required infrastructure.

**Violation signal:** Any fallback that still requires a SIM card → rethink.

---

## 3. Never Assume Continuous Connectivity

Connections drop. Nodes disappear. Design for intermittent contact, not persistent sessions.

**Rationale:** DTN (Delay-Tolerant Networking) exists because the Internet's assumption of continuous connectivity fails in challenged environments. Mobile nodes move. Battery dies. Buildings block signals. Store-carry-forward is the correct model.

**Implication:** No protocol relies on a maintained session between two peers. Every operation is idempotent or explicitly handles the case where the session drops mid-operation. Messages are self-contained and can be relayed by any node that stores them.

**Violation signal:** Any code that assumes a connection remains open for the duration of a multi-step operation → redesign with store-and-forward semantics.

---

## 4. Never Assume Every Device Supports Every Transport

iOS cannot do Wi-Fi Aware. Many Android phones lack Wi-Fi Aware. Not all phones have LoRa. Some embedded nodes have only LoRa.

**Rationale:** The device landscape is heterogeneous. A protocol that assumes all devices support all transports will silently fail on large portions of the real-world device population.

**Implication:** Capability detection is mandatory before using any transport. Transport adapters return `UNAVAILABLE` cleanly. Routing engine routes around unavailable transports without failing. Platform capability matrix maintained and tested.

**Violation signal:** Code that calls a transport API without checking `is_available()` first → bug.

---

## 5. Never Put LLM in Critical Packet-Forwarding Path

LLMs are probabilistic, slow, and require compute. Packet forwarding must be deterministic and fast.

**Rationale:** Packet forwarding decisions must be made in milliseconds with deterministic outcomes. LLMs have: multi-second inference time, probabilistic outputs (may vary on identical inputs), dependency on model file presence, high compute requirements. These properties are incompatible with real-time routing.

**Implication:** Routing decisions are deterministic algorithms (PRoPHET, Dijkstra, spray-and-wait). LLMs are engineering and operations tools: they help write code, generate documentation, analyze logs. They never touch the packet path.

**Violation signal:** Any `llm.generate()` call inside a routing, forwarding, or transport function → immediate architectural review.

---

## 6. Never Make Emergency Communication Depend on ML

ML models can be wrong, unavailable, or corrupted. Emergency routing must work with deterministic algorithms.

**Rationale:** A corrupted model file, distribution shift from training data, or inference failure could cause an emergency message to not route. This is unacceptable for P0-P1 messages where stakes are lives.

**Implication:** ML routing enhancements (if implemented) are an optimization layer on top of deterministic routing. When ML is unavailable or uncertain, the system falls back to PRoPHET/epidemic routing automatically. P0-P1 always use deterministic algorithms. ML is never the only path for any message class.

**Violation signal:** Routing function that returns "undefined" or throws when ML model unavailable → missing fallback path, fix immediately.

---

## 7. Never Invent Cryptography Casually

Custom crypto is almost always broken. Use established, peer-reviewed primitives and protocols.

**Rationale:** Cryptography is counterintuitive. Expert cryptographers have broken schemes that looked correct to their designers. The cost of a broken crypto scheme in a disaster communication system: adversary can impersonate emergency authorities, read private messages, disrupt routing.

**Implication:** Use libsodium/ring/dalek for crypto. Primitives in use:
- Ed25519 (signatures): from `ed25519-dalek` crate
- X25519 (key agreement): from `x25519-dalek` crate
- ChaCha20-Poly1305 (AEAD encryption): from `chacha20poly1305` crate
- BLAKE3 (hashing): from `blake3` crate
- HKDF-SHA256 (key derivation): from `hkdf` crate

No home-grown cipher. No obscure library without security audit. Any crypto change requires: (a) written rationale in ADR, (b) review by a cryptographer (not just a developer who has read about crypto).

**Violation signal:** A PR that introduces a new crypto dependency without ADR and cryptographer review → block.

---

## 8. Never Fabricate Benchmarks

Performance claims must come from reproducible measurements on real hardware.

**Rationale:** Fabricated or cherry-picked benchmarks lead to incorrect architectural decisions. "BLE supports 2Mbps" sounds good but achieves 100-250kbps in practice on Android due to BLE stack overhead. Making architectural decisions on theoretical numbers leads to designs that don't work.

**Implication:** Every performance claim has a linked BENCH-XXXX document with: hardware model, OS version, test method, raw data, analysis. Theoretical maximums are labeled "theoretical maximum (source: spec)" not presented as achievable throughput.

**Violation signal:** Performance claim without a BENCH-XXXX link in any document → mark as unverified, trigger experiment.

---

## 9. Never Claim Theoretical Capability as Production Capability

"BLE supports 2Mbps" ≠ "Our app achieves 2Mbps BLE throughput."

**Rationale:** Real-world constraints (OS scheduling, BLE stack implementation, background restrictions, interference, distance, connection parameters) reduce theoretical to practical. The gap is often 10x. Building on theoretical numbers produces a system that fails in the field.

**Implication:** All capability claims are from measurement or authoritative platform documentation, clearly sourced. Architecture documents use measured numbers, not spec sheet numbers. When measured numbers are unavailable, use conservative estimates and label them as estimates.

**Violation signal:** "BLE can do X" without source or measurement → challenge and require evidence.

---

## 10. Never Hide Platform Restrictions

iOS background BLE limitations are real. Android scan throttling is real. Document these clearly.

**Rationale:** Platform restrictions fundamentally shape what IRIS can do. Hiding them leads to incorrect user expectations, incorrect system design, and failure in the field.

**Implication:** Every platform adapter document has an explicit "Known Limitations" section with precise, sourced descriptions of platform restrictions. No marketing language in architecture documents. Known limitations inform use case guidance (e.g., "iOS devices should be used in foreground for active relay").

**Violation signal:** Platform document without a "Known Limitations" section → incomplete, block PR.

---

## 11. Never Ignore Legal and Regulatory Constraints

LoRa spectrum, satellite licensing, IT Act compliance, DPDPA — these are real constraints.

**Rationale:** Deploying a communication system that violates spectrum regulations, privacy law, or telecommunications law creates legal risk that can shut down the entire project. In India: WPC regulates spectrum, DOT regulates telecom services, DPDPA regulates personal data, IT Act applies to electronic communication.

**Implication:** Legal research is a first-class engineering input. Before deploying any transport in a new region, verify regulatory status. Compliance risk register maintained. Legal review required for: satellite spectrum use, LoRa power limits, data retention policies, user consent flows.

**Key constraints documented:**
- LoRa 865-867MHz: license-free ISM, max 14dBm EIRP, 1% duty cycle (India WPC rules)
- Satellite: terminal operator license required (SACFA/WPC clearance for Starlink etc.)
- DPDPA 2023: user data requires consent, data minimization, purpose limitation
- IT Act 2000 (amended): encryption use is legal; key escrow is not required

**Violation signal:** Any new transport or data practice deployed without legal clearance → stop, legal review first.

---

## 12. Never Sacrifice Safety for Autonomy

Autonomous agents must not silently modify cryptography, privacy model, or emergency systems.

**Rationale:** Autonomous code modification in high-stakes areas (cryptographic primitives, emergency routing, privacy policy) could introduce vulnerabilities or incorrect behavior that directly impacts safety. The benefit of slightly faster development iteration does not justify the risk.

**Implication:** EXECUTION_POLICY.yaml defines categories that require human approval before any autonomous agent may make changes. High-risk gates: cryptographic code, identity system, emergency authority certificates, privacy model, key management.

**Violation signal:** Autonomous agent modifies `src/crypto/`, `src/identity/`, or `src/emergency/` without human approval gate in log → audit, revert, fix policy.

---

## 13. Never Allow Documentation to Drift from Implementation

Stale docs are worse than no docs (they actively mislead).

**Rationale:** A developer reading outdated documentation makes incorrect assumptions. In a safety-critical system, incorrect assumptions can lead to bugs. Maintaining documentation is not optional overhead — it is part of the engineering process.

**Implication:** Documentation updated as part of every significant code change. Documentation accuracy is an acceptance criterion for pull requests. Architecture documents have "last verified" dates. When implementation changes, corresponding docs must change in the same PR.

**Violation signal:** PR that changes behavior without updating corresponding docs → require doc update before merge.

---

## 14. Never Call a Component Production-Ready Without Evidence

"It compiled" is not evidence. Evidence = tests passing + adversarial testing + security review + benchmarks + operational validation.

**Rationale:** Systems that aren't tested for their specific failure modes will fail in the field. For a disaster communication system, field failure has life-safety consequences.

**Acceptance criteria for COMPLETE status:**
- Unit tests passing (>80% coverage for core modules)
- Integration tests passing (multi-node scenarios)
- Adversarial tests passing (malicious relay, partition, battery failure scenarios)
- Security review completed (for any code with crypto or auth)
- Benchmarks measured and documented (for any code with performance requirements)
- Operational validation: deployed and functioning in at least one real-world scenario

**Violation signal:** Node marked COMPLETE without evidence checklist → revert status, require evidence.

---

## 15. Never Let Autonomous Agents Silently Modify High-Risk Architecture

Cryptography, identity, emergency authority, privacy model — these must have human approval gates.

**Rationale:** This extends Principle 12. High-risk areas must have explicit gates because the consequence of silent modification is catastrophic and may not be detected until deployment.

**High-risk gate categories (from EXECUTION_POLICY.yaml):**
- Any file in `src/crypto/`
- Any file in `src/identity/` or `src/keys/`
- Any change to message signature or encryption scheme
- Any change to emergency authority certificate validation
- Any change to user privacy model or data retention
- Any change to P0 message handling
- Any change to EXECUTION_POLICY.yaml itself

**Violation signal:** Git history shows autonomous commit to any of above without human approval in commit message → security incident, full audit.

---

## 16. Every Discovered Bug Becomes a Regression Test

Bugs found once will be found again without regression tests.

**Rationale:** A bug that was fixed without a test will be re-introduced when the code is refactored. For a disaster communication system, re-introduced bugs may not be caught until a disaster reveals them.

**Implication:** BUG-XXXX naming convention for significant bugs. Permanent test suite entry referencing BUG-XXXX. Test committed in the same PR as fix. Test linked to fix commit in bug report.

**Violation signal:** Bug fix PR without a new test → incomplete, request test.

---

## 17. Every Significant Architectural Decision Becomes an ADR

Design memory must outlast individuals.

**Rationale:** Team members leave. Reasoning behind decisions is forgotten. Future developers then re-make the same decisions (or the same mistakes) without knowing why the current design was chosen.

**Implication:** ADR-XXXX for every significant architectural choice. ADR index maintained. ADR required for: transport selection, protocol format choice, routing algorithm choice, security primitive choice, storage format, API design. ADR not required for: minor implementation details, local variable naming, formatting.

**Violation signal:** Major design decision made in a PR comment without ADR → require ADR before merge.

---

## 18. Every Major Uncertainty Becomes an Experiment

Guessing in architecture documents is dangerous. Experiment instead.

**Rationale:** Untested assumptions in architecture lead to systems that fail when deployed. The cost of a 2-day experiment to validate an assumption is far less than the cost of building on a wrong assumption.

**Implication:** EXP-XXXX for every technical uncertainty. Hypothesis stated before experiment. Variables controlled. Results measured. Architecture updated based on results. "I think BLE can achieve X" → EXP-XXXX to measure.

**Violation signal:** Architecture document states capability as fact without BENCH-XXXX or EXP-XXXX → mark as unverified assumption, trigger experiment.

---

## 19. Every Experiment Produces Evidence

Experiments must have measurable outcomes, not just prose conclusions.

**Rationale:** "The experiment showed it worked" is not evidence. Evidence requires data that others can review, replicate, and challenge.

**Experiment required fields:**
- Hypothesis (specific, falsifiable)
- Variables (independent, dependent, controlled)
- Method (reproducible procedure)
- Hardware/software configuration (model numbers, OS versions, IRIS version)
- Raw data (not just conclusions)
- Analysis (what the data shows and why)
- Conclusion (was hypothesis supported? what changed in architecture?)

**Violation signal:** EXP-XXXX marked COMPLETE without raw data table → reject, require data.

---

## 20. Every Meaningful Discovery Updates the Project Graph

The graph must reflect reality, not initial assumptions.

**Rationale:** The project plan was made before the engineering was done. Engineering discovers facts that change the plan. If the graph is not updated, the plan becomes fiction, and work proceeds on false assumptions.

**Implication:** Node discovery is a continuous process. New nodes created when evidence reveals a required component or decision not previously captured. Node status updated when experiments change understanding. The graph is a living document, not a one-time deliverable.

**Violation signal:** Experiment reveals a new required component but no new node is created → update graph.

---

## Principle Priority Under Conflict

When principles conflict, resolve by asking: "Which choice preserves emergency communication capability?"

Safety-preserving order for conflicts:
1. Life-safety first: P0-P1 message delivery above all else
2. Security: do not compromise cryptography for features
3. Correctness: a correct slow system beats a fast broken one
4. Performance: optimize after correctness is established
5. Development convenience: never traded for the above
