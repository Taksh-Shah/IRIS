# CHECKPOINT.md — Latest Durable Checkpoint

**Schema version**: 1.0
**Checkpoint ID**: CHK-0007
**Timestamp**: 2026-09-06T09:30:00+05:30
**Agent**: Codex — documentation/state reconciliation

> **Current checkpoint:** The August control-plane checkpoint below is retained
> as historical bootstrap evidence. The active state is the physical-device
> phase: two phones are connected, the shipping-app Tier-2 Wi-Fi Direct gate is
> 10/10, and the Mobly Tier-2 harness remains HW-PENDING on Vivo/OriginOS.

## Current reconciliation

- Repository head: `b7a876c`; worktree clean before this documentation pass.
- P1: vivo V2205, Android 14/API 34; P2: vivo 2004, Android 12/API 31.
- Host tools: JDK 21, Python 3.10, Mobly 1.12.2, ADB 37.0.1.
- Documentation/state projections refreshed without changing implementation,
  APKs, or device state.

---

## Mission

Build the IRIS Autonomous Engineering Control Plane to enable OpenCode + LLM to execute the IRIS engineering graph autonomously, surviving context compaction.

## Current Node

PROTO-001 (Protocol Design) — DESIGNING

## Last Completed Action

Built complete IRIS Autonomous Engineering Control Plane:
- AGENTS.md bootloader
- engineering/memory/ durable state system (12 files)
- engineering/memory/records/ addressable records system (8 directories)
- .opencode/ directory structure with commands, agents, skills, plugins
- OpenCode compaction resilience plugin
- 11 slash commands
- 11 specialized agents
- 3 skills
- Graph validator and state validator
- Context builder system
- opencode.jsonc configuration

## Current Action

Control plane implementation complete. System ready for protocol design execution.

## Next Action

1. Verify P0 SOS byte budget (docs/protocol/PROTOCOL_OVERVIEW.md)
2. Validate protocol document consistency
3. Identify missing protocol specifications
4. Write protocol test vector specification
5. Update PROTO-001 in graph
6. Mark COMPLETE if criteria met

## Modified Files

New files created:
- AGENTS.md
- engineering/memory/*.md (12 files)
- engineering/memory/records/*/ (8 directories)
- .opencode/commands/*.md (11 files)
- .opencode/agents/*.md (11 files)
- .opencode/skills/*.md (3 files)
- .opencode/plugins/compaction-resilience.js
- opencode.jsonc

## Decisions Made

- DEC-0007: Control Plane Architecture approved
- State file format: Markdown with YAML frontmatter
- Record format: Individual files per record in subdirectories
- Plugin: Use experimental.session.compaction hook
- Agents: 11 specialized agents with small prompts inheriting shared rules

## Blockers

- BLK-0001: Cryptographic architecture unresolved (CRITICAL, blocks CRYPTO-001)
- BLK-0002: Graph size insufficient (MEDIUM, not blocking current work)

## Discoveries

- DISC-0001: Ed25519 SHA-512 internal (not configurable)
- DISC-0002: HKDF-BLAKE3 non-standard
- DISC-0003: No forward secrecy against recipient compromise
- DISC-0004: LoRa duty cycle applies to all messages
- DISC-0005: India LoRa spectrum 865–868 MHz
- DISC-0006: iOS cannot relay Wi-Fi Aware/Direct
- DISC-0007: P0 SOS fits in 255 bytes

## Failed Approaches

- FAIL-0001: P0 SOS ignores LoRa duty cycle (regulatory violation)
- FAIL-0002: Claiming IRIS engaged legal counsel (fabricated fact)
- FAIL-0003: Ed25519 uses BLAKE3 internally (incorrect algorithm claim)

## Tests Run

None (control plane build — no protocol code yet)

## Verification State

- Graph structure: VALIDATED
- State files: ALL 12 CREATED AND VERIFIED
- Plugin structure: CREATED (compaction-resilience.js)
- Commands: ALL 11 CREATED (resume, checkpoint, status, graph, next, verify, health, research, decision, accept, discover)
- Agents: ALL 11 CREATED (orchestrator, architect, researcher, protocol, security, tester, redteam, verifier, graph-manager, state-manager, documentation)
- Skills: ALL 3 CREATED (context-builder, graph-validator, checkpoint-recovery)
- Configuration: opencode.jsonc CREATED
- AGENTS.md bootloader: CREATED
- All 42 control plane files verified present

## Graph Transition

No transition yet. PROTO-001 remains active.

## Unresolved Questions

- OQ-0001 through OQ-0008 (see OPEN_QUESTIONS.md)

---

## Control Plane File Inventory

### Durable State (12 files)
- engineering/memory/CURRENT_STATE.md
- engineering/memory/CURRENT_MISSION.md
- engineering/memory/ACTIVE_NODE.md
- engineering/memory/NEXT_ACTION.md
- engineering/memory/LAST_ACTION.md
- engineering/memory/DECISIONS.md
- engineering/memory/BLOCKERS.md
- engineering/memory/OPEN_QUESTIONS.md
- engineering/memory/DISCOVERIES.md
- engineering/memory/FAILED_APPROACHES.md
- engineering/memory/CHECKPOINT.md
- engineering/memory/SESSION_STATE.md

### Addressable Records (2 files + 8 directories)
- engineering/memory/records/README.md
- engineering/memory/records/ALLOCATION.md
- engineering/memory/records/decisions/
- engineering/memory/records/research/
- engineering/memory/records/experiments/
- engineering/memory/records/failures/
- engineering/memory/records/discoveries/
- engineering/memory/records/changes/
- engineering/memory/records/verification/
- engineering/memory/records/checkpoints/

### Commands (11 files)
- .opencode/commands/resume.md
- .opencode/commands/checkpoint.md
- .opencode/commands/status.md
- .opencode/commands/graph.md
- .opencode/commands/next.md
- .opencode/commands/verify.md
- .opencode/commands/health.md
- .opencode/commands/research.md
- .opencode/commands/decision.md
- .opencode/commands/accept.md
- .opencode/commands/discover.md

### Agents (11 files)
- .opencode/agents/orchestrator.md
- .opencode/agents/architect.md
- .opencode/agents/researcher.md
- .opencode/agents/protocol.md
- .opencode/agents/security.md
- .opencode/agents/tester.md
- .opencode/agents/redteam.md
- .opencode/agents/verifier.md
- .opencode/agents/graph-manager.md
- .opencode/agents/state-manager.md
- .opencode/agents/documentation.md

### Skills (3 files)
- .opencode/skills/context-builder.md
- .opencode/skills/graph-validator.md
- .opencode/skills/checkpoint-recovery.md

### Plugin (1 file)
- .opencode/plugins/compaction-resilience.js

### Configuration (1 file)
- opencode.jsonc

### Bootloader (1 file)
- AGENTS.md

### Total: 42 files
