# Incident Management — Detection, Response, and Post-Mortem

**Component:** Operations
**Status:** Policy v1.0
**Last Updated:** 2026-08-11

---

## 1. Incident Severity Definitions

### P0 — Critical (Network Safety)

Incidents where the IRIS network may fail to deliver life-safety messages. Requires immediate response.

| Example P0 incidents |
|---|
| P0 (SOS) message non-delivery confirmed |
| Network-wide partition affecting > 50% of nodes |
| Cryptographic subsystem failure |
| Routing engine crash affecting all nodes |
| Evidence of active attack on network integrity |

**Response SLA:** Acknowledge within 15 minutes. First remediation action within 1 hour.

### P1 — High (Significant Degradation)

| Example P1 incidents |
|---|
| P1 (Medical) message delivery ratio drops below 70% |
| Single edge server failure in a critical location |
| Battery failure on a bridge node |
| Satellite uplink lost for > 30 minutes |
| Storage exhaustion on edge server |

**Response SLA:** Acknowledge within 1 hour. First remediation action within 4 hours.

### P2 — Medium (Reduced Performance)

| Example P2 incidents |
|---|
| Delivery ratio degraded but above acceptable threshold |
| Single transport failure on a non-critical node |
| ML routing disabled (falls back to PRoPHET) |
| Log export failure |

**Response SLA:** Acknowledge within 4 hours. Resolution within 24 hours.

### P3 — Low (Minor Issues)

| Example P3 incidents |
|---|
| Non-critical performance degradation |
| Cosmetic UI issues |
| Documentation errors |
| Non-urgent configuration drift |

**Response SLA:** Acknowledge within 24 hours. Resolution within 7 days.

---

## 2. Incident Response Procedure

### Phase 1: Detect

Incidents are detected via:

- **Automated alerting:** IRIS coordinator dashboard raises alerts when delivery ratio drops, errors spike, or critical thresholds are crossed
- **User report:** Responder reports non-delivery or system error
- **Log monitoring:** Automated log analysis on edge server detects ERROR or spike in WARN events
- **Heartbeat failure:** Edge server stops sending heartbeat to coordinator for > 5 minutes

When a potential incident is detected, the responder creates an incident record (Section 4) immediately, even before full confirmation.

### Phase 2: Triage

**Triage questions:**
1. Is P0 message delivery affected? (If yes → P0 incident regardless of other factors)
2. How many nodes are affected?
3. Is this a hardware failure, software failure, or configuration error?
4. Is the issue expanding or stable?

**Triage tool:**

```bash
# Quick network health check from edge server
iris-cli diagnostics network-health

# Output example:
# P0 delivery ratio (10 min): 0.94  OK
# P1 delivery ratio (10 min): 0.87  OK
# Active nodes: 67/71        WARN (4 nodes not seen in 15 min)
# Bridge nodes at risk: 1    WARN (node sha256:a3f7 battery < 20%)
# Network partitions: 1      ALERT (2 isolated nodes detected)
```

### Phase 3: Contain

Immediate containment actions prevent the incident from worsening:

| Incident type | Containment action |
|---|---|
| Network partition | Identify and physically move a relay device to bridge the gap |
| Edge server failure | Activate backup edge server (pre-staged in hardened kit) |
| Battery failure | Connect to AC power or swap battery |
| Routing engine crash | Restart IRIS service: `systemctl restart iris` |
| ML routing interference | Disable ML routing: `iris-cli config set ml_routing_enabled false` |
| Storage exhaustion | Archive old messages: `iris-cli store archive --older-than 24h` |
| Active attack | Isolate affected node: `iris-cli network isolate-node <node_id_hash>` |

### Phase 4: Resolve

Resolution restores full functionality. Resolution steps depend on root cause (identified during containment). Key resolution milestones:

- P0 delivery ratio returns to ≥ 95%
- Affected nodes reconnected
- Root cause identified and documented
- Monitoring confirms stability for ≥ 30 minutes

The incident is marked **resolved** only when the coordinator confirms normal operation. Do not close incidents prematurely.

### Phase 5: Post-Mortem

Post-mortem is required for all P0 and P1 incidents. Optional but encouraged for P2.

**Timeline:** Post-mortem document must be completed within 48 hours of incident resolution.

---

## 3. Escalation Path

IRIS uses role-based escalation (not named individuals, to support team rotation):

| Level | Role | When to escalate |
|---|---|---|
| L1 | Field Responder | First point of contact; handle P2–P3 |
| L2 | IRIS Network Coordinator | Escalate P1–P0; coordinate cross-team response |
| L3 | IRIS Technical Lead (on-call) | Escalate if coordinator cannot resolve P0 within 1 hour |
| L4 | IRIS Engineering Team | Escalate software bugs, security incidents |

Escalation method: Radio contact first (P0); satellite message if radio unavailable; cellular if available.

**Escalation message format:**
```
IRIS INCIDENT [P0/P1] [timestamp]
Node: [unit ID]
Issue: [brief description]
Impact: [affected nodes/users]
Containment: [actions taken so far]
Need: [specific request from next level]
```

---

## 4. Incident Register

All incidents are recorded in the incident register. Format:

| Field | Description |
|---|---|
| `INC-ID` | Unique ID: INC-YYYY-NNNN (e.g., INC-2026-0042) |
| `Date` | ISO 8601 date of incident detection |
| `Time detected` | UTC time |
| `Severity` | P0 / P1 / P2 / P3 |
| `Detected by` | Role (not name): field_responder, automated_alert, coordinator |
| `Affected nodes` | Count and pseudonymized IDs |
| `Affected users` | Estimated count |
| `Description` | One-sentence description |
| `Root cause` | Short root cause (filled at resolution) |
| `Resolution time` | Duration from detection to resolution |
| `Post-mortem` | Link to post-mortem document (P0/P1 only) |
| `Status` | OPEN / RESOLVED / CLOSED |

### Example Incident Record

```
INC-ID:          INC-2026-0042
Date:            2026-08-11
Time detected:   14:32 UTC
Severity:        P0
Detected by:     automated_alert (p0_delivery_ratio_low)
Affected nodes:  12 (sha256:a3f7, sha256:b7c2, ...)
Affected users:  ~85
Description:     LoRa bridge node battery failure caused network partition;
                 3 P0 SOS messages undelivered for 23 minutes.
Root cause:      Solar panel shading from debris; battery depleted.
Resolution time: 47 minutes
Post-mortem:     docs/incidents/PM-2026-0042.md
Status:          CLOSED
```

---

## 5. Post-Mortem Template

```markdown
# Post-Mortem: INC-2026-NNNN

**Incident ID:** INC-2026-NNNN
**Severity:** P0
**Date:** YYYY-MM-DD
**Duration:** HH:MM
**Author:** [Role, not name]
**Reviewers:** [Roles]

## Summary

[2-3 sentence summary of what happened and why.]

## Timeline

| Time (UTC) | Event |
|---|---|
| 14:32 | Automated alert: P0 delivery ratio < 0.5 |
| 14:35 | L2 coordinator engaged |
| 14:48 | Root cause identified: battery failure on sha256:a3f7 |
| 15:00 | Battery swapped, node reconnected |
| 15:19 | P0 delivery ratio restored to 0.97 |
| 15:32 | Incident declared resolved |

## Impact

- P0 SOS messages affected: 3 (all eventually delivered after 47 min)
- P1 messages delayed: 12
- Nodes isolated: 23 (33% of network)
- Duration of degradation: 47 minutes

## Root Cause

[Specific technical root cause. Not "human error" — what system or process failure allowed the human error to have impact?]

## Contributing Factors

1. [Factor 1]
2. [Factor 2]

## What Went Well

- [What the team did correctly during response]

## What Could Be Improved

- [Specific, actionable improvement]

## Action Items

| Action | Owner (role) | Due date | Status |
|---|---|---|---|
| Install battery level monitoring alert at 30% | IRIS Technical Lead | 2026-08-18 | OPEN |
| Add solar panel shading check to deployment checklist | Field Operations Lead | 2026-08-15 | OPEN |

## Lessons Learned

[1-2 paragraphs on systemic lessons.]
```

---

## 6. Automated Detection Rules

The IRIS coordinator runs these automated detection checks every 60 seconds:

| Rule | Condition | Severity raised |
|---|---|---|
| P0 delivery ratio | < 0.80 for 5 consecutive minutes | P0 |
| P1 delivery ratio | < 0.60 for 10 consecutive minutes | P1 |
| Network partition | `connected_components > 1` for > 5 minutes | P1 |
| Bridge node battery | Battery < 20% on any bridge node | P1 |
| Edge server offline | Heartbeat missing for > 10 minutes | P1 |
| Queue critical | Any node queue > 95% for > 5 minutes | P2 |
| ERROR rate spike | > 10 ERROR events per minute | P2 |

Detection rules are configured in `iris-edge/config/alerting.yaml`.

---

## 7. References

- IRIS diagnostics: `docs/operations/DIAGNOSTICS.md`
- IRIS logging: `docs/operations/LOGGING.md`
- AI safety incidents: `docs/intelligence/AI_SAFETY.md`
- Field operations: `docs/operations/FIELD_OPERATIONS.md`
