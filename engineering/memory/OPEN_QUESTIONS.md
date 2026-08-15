# OPEN_QUESTIONS.md

**Schema version**: 1.0
**Last updated**: 2026-08-11T19:30:00Z

---

## Open Questions

### OQ-0001: Does IRIS qualify as Significant Social Media Intermediary?
**Domain**: Legal
**Severity**: HIGH
**Node**: LEGAL-001
**Description**: Under IT Rules 2021, does IRIS at scale qualify as SSMI? What compliance obligations follow?
**Status**: UNRESOLVED
**Resolution**: Requires Indian telecom lawyer review

### OQ-0002: Does message relay require telecom license in India?
**Domain**: Legal
**Severity**: HIGH
**Node**: LEGAL-001
**Description**: If IRIS relays messages between users, does this constitute a telecom service requiring license?
**Status**: UNRESOLVED
**Resolution**: Requires Indian telecom lawyer review

### OQ-0003: What type approval is required for LoRa modules in India?
**Domain**: Regulatory
**Severity**: MEDIUM
**Node**: LEGAL-001
**Description**: WPC type approval requirements for LoRa modules sold/deployed in India
**Status**: UNRESOLVED
**Resolution**: WPC gazette research + potential BIS certification check

### OQ-0004: Satellite terminal operation licensing requirements
**Domain**: Regulatory
**Severity**: MEDIUM
**Node**: SAT-001
**Description**: License requirements for operating satellite terminals (Iridium GO!, VSAT) in India
**Status**: UNRESOLVED
**Resolution**: DoS/IN-SPACe regulations research

### OQ-0005: iOS background BLE relay capability
**Domain**: Technical
**Severity**: MEDIUM
**Node**: BLE-002
**Description**: What is the practical BLE relay capability on iOS 14+ given background execution limits?
**Status**: PLANNED_EXPERIMENT
**Experiment**: EXP-005
**Resolution**: Physical device testing needed

### OQ-0006: Optimal routing algorithm for India disaster mobility
**Domain**: Technical
**Severity**: MEDIUM
**Node**: ROUTE-001
**Description**: How well does PRoPHET perform with Indian disaster-specific mobility patterns?
**Status**: PLANNED_EXPERIMENT
**Experiment**: EXP-002
**Resolution**: Simulation with India-specific mobility traces

### OQ-0007: Battery drain of continuous BLE relay on Indian market phones
**Domain**: Technical
**Severity**: MEDIUM
**Node**: BLE-001
**Description**: Measured battery impact on Redmi, Realme, Samsung A-series phones common in India
**Status**: PLANNED_EXPERIMENT
**Experiment**: EXP-003
**Resolution**: Physical device testing needed

### OQ-0008: LoRa performance in dense urban India
**Domain**: Technical
**Severity**: MEDIUM
**Node**: LORA-001
**Description**: What is the practical LoRa range in dense urban Indian environments?
**Status**: REQUIRES_HARDWARE
**Resolution**: Physical LoRa module testing in target environment

---

## Resolved Questions

None yet.

---

## Question Record Format

```
OQ-XXXX: [Question]
Domain: Legal | Regulatory | Technical | Security
Severity: HIGH | MEDIUM | LOW
Node: XXXX-XXX
Description: [Full question]
Status: UNRESOLVED | PLANNED_EXPERIMENT | REQUIRES_HARDWARE | RESOLVED
Resolution: [How to resolve]
```
