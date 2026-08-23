# IRIS Carried Obligations Register — LEGAL-001 deferral (2026-08-22)

**Decision**: LEGAL-001 is **not required at this stage** (operator ruling,
iter ~178). The node is moved to the FUTURE backlog with status
`DEFERRED_TO_FUTURE`. Its purpose (preventing misuse by hostile actors) is
preserved through the technical controls already shipped in code — SEC-001,
EMERG-001, IDENT-001, and every transport-level hard gate — while the pure
legal-execution items below are deferred until deployment planning begins.

**Reactivation trigger**: any of — field pilot with TX-capable units; India
market release; government/NGO MoU signature; hardware procurement for
satellite/LoRa modems.

| # | Obligation | Source | Reactivation gate | Owner |
|---|---|---|---|---|
| O-1 | WPC-ETA (type approval) per-SKU for LoRa radios | RES-0027 RQ-1 / REG_NOTES | LoRa hardware procurement | Operator + counsel |
| O-2 | Authorized GMPCS/MSS airtime channel for satellite TX | RES-0028 RQ-6 / DEC-SAT-0006 | Any satellite field activation (India) | Operator + counsel |
| O-3 | Telecom Act 2023 compliance review (satphone possession rules) | RES-0028 FC-4 | India deployment of any satcom device | Counsel |
| O-4 | STQC certification path | REG_NOTES Month-30 item | Government-facing rollout | Operator |
| O-5 | NDRF MoU / B2B contract structure | PILOT_001 docs | Pilot engagement | Operator + counsel |
| O-6 | IT Rules 2021 applicability review | LEGAL research record | Public-facing service launch | Counsel |

**Standing technical controls active TODAY** (independent of this deferral):
envelope signature verification on every inbound message; P3+ satellite hard
gate; WPC duty-cycle enforcement with no override knob; spend guards;
rate-limiter/quota/spam/reputation engines; authority-chain ACLs; drill-mode
emergency separation.

No legal opinion is rendered by this register or by any project artifact.
