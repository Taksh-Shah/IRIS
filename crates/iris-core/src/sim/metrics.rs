//! Outcome metrics — SIM-001 (WP-6).
//!
//! Aggregations over [`crate::sim::SimOutcome`]: delivery ratio per priority,
//! latency histogram, storage utilization, and eviction counts. These are the
//! acceptance metrics for the ≥3 reproducible sim scenarios.

use std::collections::HashMap;

use crate::message::MessagePriority;
use crate::sim::SimOutcome;

/// Per-priority delivered/injected counts plus derived ratio.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PrioritySlice {
    pub delivered: usize,
    pub injected: usize,
}

impl PrioritySlice {
    pub fn ratio(&self) -> f64 {
        if self.injected == 0 {
            0.0
        } else {
            self.delivered as f64 / self.injected as f64
        }
    }
}

/// Full metrics snapshot for one simulation run.
#[derive(Debug, Clone)]
pub struct SimMetrics {
    pub injected_total: usize,
    pub delivered_total: usize,
    pub delivery_ratio: f64,
    pub per_priority: HashMap<MessagePriority, PrioritySlice>,
    pub latency_p50_ms: u64,
    pub latency_p95_ms: u64,
    /// Max buffered bytes observed across nodes (proxy for storage util).
    pub peak_storage_bytes: u64,
    pub evictions_total: u64,
    pub max_hops_seen: u8,
    /// ROUTE-002: relay transmissions / delivered (ONE overhead ratio).
    pub overhead_ratio: f64,
    /// ROUTE-002: average hop count across tracked messages.
    pub avg_hop_count: f64,
    pub loop_free: bool,
    pub result_anchor: String,
}

impl SimMetrics {
    pub fn from_outcome(out: &SimOutcome) -> Self {
        let mut per_priority: HashMap<MessagePriority, PrioritySlice> = HashMap::new();
        for (prio, (d, i)) in &out.per_priority {
            per_priority.insert(
                *prio,
                PrioritySlice {
                    delivered: *d,
                    injected: *i,
                },
            );
        }

        let mut lat = out.latencies.clone();
        lat.sort_unstable();
        let (p50, p95) = percentiles(&lat, 50.0, 95.0);

        // SIM-6: `out.peak_storage_bytes` is the true running high-water
        // mark, sampled every tick during the run — recomputing it here
        // from `out.nodes` would only see *final* usage (the bug this
        // finding describes: a partition that buffers everything then
        // heals right before the run ends reports near-empty storage).
        let peak_storage_bytes = out.peak_storage_bytes;
        // Already summed once in `run()`; re-summing `n.evictions` here was
        // a redundant duplicate of the same computation, harmless but
        // pointless now that this block is being touched anyway.
        let evictions_total = out.evictions_total;

        SimMetrics {
            injected_total: out.injected_total,
            delivered_total: out.delivered_total,
            delivery_ratio: out.delivery_ratio,
            per_priority,
            latency_p50_ms: p50,
            latency_p95_ms: p95,
            peak_storage_bytes,
            evictions_total,
            max_hops_seen: out.max_hops_seen,
            // SIM-18: 0.0 is the *best* possible overhead — using it as the
            // zero-delivery sentinel made a catastrophic-failure run (many
            // relays, nothing delivered) indistinguishable from a perfect
            // one. Lower is better, so the undefined case must be
            // pessimistic: `INFINITY` composes correctly with a direct
            // `<` comparison (SIM-17) without requiring callers to unwrap
            // an `Option` first, and any finite overhead sorts below it.
            overhead_ratio: if out.delivered_total == 0 {
                f64::INFINITY
            } else {
                out.relays_total as f64 / out.delivered_total as f64
            },
            avg_hop_count: out.avg_hop_count,
            loop_free: out.loop_free(),
            result_anchor: out.result_anchor.clone(),
        }
    }

    /// Compact one-line summary for the verification doc.
    pub fn one_line(&self) -> String {
        format!(
            "deliver {}/{} ({:.1}%), p50 {}ms p95 {}ms, hops {}",
            self.delivered_total,
            self.injected_total,
            self.delivery_ratio * 100.0,
            self.latency_p50_ms,
            self.latency_p95_ms,
            self.max_hops_seen,
        )
    }

    /// Delivery ratio for one priority class.
    pub fn delivered_ratio_for(&self, prio: MessagePriority) -> f64 {
        self.per_priority
            .get(&prio)
            .map(|s| s.ratio())
            .unwrap_or(0.0)
    }

    /// Overhead ratio (relays / delivered) — ONE methodology parity.
    pub fn overhead(&self) -> f64 {
        self.overhead_ratio
    }
}

fn percentiles(sorted: &[u64], p50: f64, p95: f64) -> (u64, u64) {
    if sorted.is_empty() {
        return (0, 0);
    }
    let at = |p: f64| {
        let idx = ((sorted.len() as f64 - 1.0) * p / 100.0).round() as usize;
        sorted[idx]
    };
    (at(p50), at(p95))
}

/// Wilson score interval at 95% confidence for a binomial proportion —
/// PILOT-001 KPI evaluation (docs/operations/PILOT_KPI_PLAN.md §1). Returns
/// `(lower, upper)`. Small-N safe; sampling is per class/transport per leg
/// (never pooled) — the interval's lower bound is the pass/fail statistic.
pub fn wilson_ci_95(k: usize, n: usize) -> (f64, f64) {
    let n = n as f64;
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let z = 1.96; // two-tailed 95%
    let p = k as f64 / n;
    let z2 = z * z;
    let centre = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let margin = (z * ((p * (1.0 - p)) / n + z2 / (4.0 * n * n)).sqrt()) / (1.0 + z2 / n);
    (
        (centre - margin).clamp(0.0, 1.0),
        (centre + margin).clamp(0.0, 1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_edge_cases() {
        assert_eq!(percentiles(&[], 50.0, 95.0), (0, 0));
        assert_eq!(percentiles(&[5], 50.0, 95.0), (5, 5));
        assert_eq!(percentiles(&[1, 2, 3, 4], 50.0, 95.0), (3, 4));
    }

    #[test]
    fn priority_slice_ratio() {
        let s = PrioritySlice {
            delivered: 2,
            injected: 4,
        };
        assert!((s.ratio() - 0.5).abs() < 1e-9);
        assert_eq!(
            PrioritySlice {
                delivered: 0,
                injected: 0
            }
            .ratio(),
            0.0
        );
    }

    #[test]
    fn wilson_ci_95_full_samples_bind() {
        // k == n: perfect delivery still gets a finite lower bound (0.886 for
        // n=30, 0.929 for n=50, 0.963 for n=100) and an upper bound clamped to
        // 1.0. The pilot's B-1 floor is met statistically at N >= 50.
        let (lo, hi) = wilson_ci_95(30, 30);
        assert!(
            lo > 0.85 && lo < 0.90 && hi <= 1.0 + 1e-9,
            "lo={lo} hi={hi}"
        );
        // k == 0: failure always keeps a small upper bound.
        let (lo0, hi0) = wilson_ci_95(0, 50);
        assert!(lo0.abs() < 1e-9 && hi0 < 0.08, "lo0={lo0} hi0={hi0}");
        // Monotone in k and in n (perfect delivery tightens with more samples).
        let (a, _) = wilson_ci_95(40, 50);
        let (b, _) = wilson_ci_95(50, 50);
        let (c, _) = wilson_ci_95(100, 100);
        assert!(a < b && b < c);
        // Empty sample is degenerate, does not panic.
        assert_eq!(wilson_ci_95(0, 0), (0.0, 0.0));
    }
}
