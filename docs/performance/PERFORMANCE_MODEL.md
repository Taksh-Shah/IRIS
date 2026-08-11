# Performance Model — Analytical Framework for IRIS Metrics

**Component:** Analysis / Design Reference
**Status:** Design v1.0
**Last Updated:** 2026-08-11

---

## 1. Overview

This document provides an analytical performance model for IRIS — mathematical relationships between network parameters and observed metrics. The model serves three purposes:

1. **Design guidance:** Predict performance before building, to make architectural choices
2. **Simulation validation:** Verify simulator results against analytical expectations
3. **Operational planning:** Predict behavior when scaling a deployment

Where closed-form analysis is intractable, simulation is the authoritative source. Section 5 defines when each approach is appropriate.

---

## 2. Key Metrics

| Metric | Symbol | Definition |
|---|---|---|
| Delivery ratio | DR | Fraction of messages delivered before TTL expiry |
| Priority delivery ratio | DR_p | DR restricted to priority class p |
| End-to-end latency | L | Time from message creation to delivery (seconds) |
| Battery lifetime | B | Hours of operation at given traffic load |
| Throughput | T | Useful data delivered per unit time per node |
| Overhead ratio | OR | (relayed messages − original messages) / original messages |

---

## 3. Delivery Ratio Model

### 3.1 Epidemic Routing (Upper Bound)

For a network of `n` nodes with uniform contact rate `λ` (contacts/node/second) and TTL `t`:

```
DR_epidemic = 1 - exp(-λ(n-1)t)
```

This is the upper bound — no routing algorithm can exceed this for given `(n, λ, t)`.

**Example:** n=100, λ=1/3600 (one contact per hour), t=4×3600 (4-hour TTL):
```
DR_epidemic = 1 - exp(-1/3600 × 99 × 14400) = 1 - exp(-396) ≈ 1.0
```

For sparser scenarios: n=20, λ=1/7200 (one contact per 2 hours), t=7200:
```
DR_epidemic = 1 - exp(-1/7200 × 19 × 7200) = 1 - exp(-19) ≈ 1.0
```

Epidemic routing achieves near-100% delivery even in sparse networks if TTL is generous. The cost is overhead: every node carries every message.

### 3.2 PRoPHET Routing (Empirical Model)

PRoPHET delivery ratio is not analytically tractable in closed form. Empirical relationships from ONE simulator benchmarks:

```
DR_prophet ≈ DR_epidemic × efficiency_factor
efficiency_factor ≈ 0.75 - 0.95  (depends on mobility and contact regularity)
```

Higher efficiency factors occur when contact patterns are regular (same nodes meet at predictable times). Lower efficiency when contacts are highly random.

### 3.3 Spray-and-Wait (Analytical)

For Spray-and-Wait with L copies in a network of n nodes:

```
DR_spw(L) ≈ 1 - (1 - 1/n)^L × exp(-λ × L × t)
```

Approximation valid for L << n and uniform contact rates. For L=20, n=100, λ=1/3600, t=7200:
```
DR_spw ≈ 1 - (0.99)^20 × exp(-20/3600 × 7200) = 1 - 0.818 × 0.018 = 0.985
```

### 3.4 P0 Delivery Ratio as Function of Parameters

The target DR for P0 is > 0.99 for direct delivery (1 hop). Analytical condition for direct delivery to succeed:

```
P(at least one contact within 30s) = 1 - exp(-λ × 30)

For P ≥ 0.99:
  λ ≥ -ln(0.01) / 30 ≈ 0.154 contacts/second
```

This means P0 direct delivery requires a contact rate of at least 0.154/s — only achievable in dense environments. In practice, P0 uses epidemic flooding to all currently-connected peers.

---

## 4. Latency Model

### 4.1 Direct Delivery Latency

For a direct delivery (no relay), latency is:

```
L_direct = t_queue + t_transport
```

Where:
- `t_queue` = time waiting in priority queue (P0: < 1s by design)
- `t_transport` = transfer time on transport medium

**Transport latency characteristics:**

| Transport | Connection setup | Transfer time (1 KB) | Total (1 KB) |
|---|---|---|---|
| BLE 5.x | 20–100 ms | 10 ms | 30–110 ms |
| Wi-Fi Direct | 1–5 s | 1 ms | 1–5 s |
| LoRa (SF9) | < 1 ms | 144 ms | 145 ms |
| Cellular | 1–3 s | < 10 ms | 1–3 s |
| Satellite (Iridium) | 5–60 s | 2 s | 7–62 s |

### 4.2 Multi-Hop Latency

For a message that traverses k hops:

```
L_khop = Σ_{i=1}^{k} (t_wait_i + t_transfer_i)
```

where `t_wait_i` is the time the message waits at node i for the next contact with an appropriate relay.

For exponentially distributed inter-contact times with mean `μ`:

```
E[t_wait_i] = μ/2  (expected wait time = half the mean inter-contact time)
```

**P0 latency budget derivation:**

| Path | Budget | Feasibility |
|---|---|---|
| Direct (0 hops) | 30 s | Achievable with BLE/Wi-Fi on contact |
| 1-hop relay | 5 min = 300 s | Requires inter-contact time < 600 s (10 min) |
| Multi-hop (5 hops) | 30 min = 1,800 s | Requires avg 360 s/hop including transfer |

---

## 5. Battery Model

### 5.1 Battery Life Equation

```
T_battery = C_battery / I_avg

I_avg = I_idle + Σ_transport (duty_fraction_t × I_t)
```

Where:
- `C_battery` = battery capacity (mAh)
- `I_idle` = baseline CPU/memory current (Android: ~50 mA, RPi Zero: ~100 mA)
- `duty_fraction_t` = fraction of time transport is active
- `I_t` = current draw when transport active

### 5.2 Current Draw by Transport

| Transport | State | Current (mA) |
|---|---|---|
| BLE | Advertising (1 Hz) | ~2 |
| BLE | Scanning (active) | ~20 |
| BLE | Connected data transfer | ~30 |
| Wi-Fi Direct | Idle/listening | ~15 |
| Wi-Fi Direct | Data transfer | ~100 |
| LoRa | Receive mode | ~12 |
| LoRa | Transmit (14 dBm) | ~120 |
| Cellular (LTE) | Idle | ~10 |
| Cellular (LTE) | Data transfer | ~300 |

### 5.3 Example Battery Budget (Android, 4,000 mAh)

Typical disaster scenario: BLE advertising continuously, active scan 10% of time, LoRa RX 50% of time, LoRa TX 1% of time:

```
I_avg = 50 (base)
      + 2 (BLE advertising)
      + 0.10 × 20 (BLE scan)
      + 0.50 × 12 (LoRa RX)
      + 0.01 × 120 (LoRa TX)
      = 50 + 2 + 2 + 6 + 1.2
      = 61.2 mA

T_battery = 4000 / 61.2 ≈ 65 hours
```

Target: 24 hours at 20% battery drain (800 mAh). Current budget 61.2 mA × 24h = 1,469 mAh — fails the 20% drain target. Mitigation: reduce BLE scan duty cycle and LoRa RX duty cycle in background mode.

---

## 6. Analytical Model vs Simulation: When to Use Each

| Question | Use analytical model? | Use simulation? |
|---|---|---|
| Order-of-magnitude delivery ratio estimate | Yes | Optional |
| Exact delivery ratio for specific scenario | No | Yes |
| Battery lifetime estimate | Yes (within ±20%) | Optional |
| Routing algorithm comparison | No | Yes |
| Optimal L-value for Spray-and-Wait | Yes (closed-form approximation) | Validate |
| LoRa link budget | Yes | Validate |
| Bloom filter saturation threshold | Yes (closed-form) | Optional |
| Performance under India-specific mobility | No | Yes |
| Impact of network partition events | No | Yes |

**Rule of thumb:** Use the analytical model for initial design choices and to validate simulation results. Use simulation for quantitative performance claims.

---

## 7. References

- Epidemic routing theory: Groenevelt et al. (2005)
- PRoPHET benchmarks: ONE simulator Helsinki scenario
- Spray-and-Wait analysis: Spyropoulos et al. (2005), "Spray and Wait: An Efficient Routing Scheme for Intermittently Connected Mobile Networks"
- Battery model methodology: IRIS hardware measurements (internal)
- Simulation: `docs/simulation/SIMULATION_VALIDATION.md`
