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
            per_priority.insert(*prio, PrioritySlice { delivered: *d, injected: *i });
        }

        let mut lat = out.latencies.clone();
        lat.sort_unstable();
        let (p50, p95) = percentiles(&lat, 50.0, 95.0);

        let mut peak_storage_bytes = 0u64;
        let mut evictions_total = 0u64;
        for n in &out.nodes {
            let used = n.scf.usage_bytes();
            peak_storage_bytes = peak_storage_bytes.max(used);
            evictions_total += n.evictions;
        }

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
            overhead_ratio: if out.delivered_total == 0 {
                0.0
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
        self.per_priority.get(&prio).map(|s| s.ratio()).unwrap_or(0.0)
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
        let s = PrioritySlice { delivered: 2, injected: 4 };
        assert!((s.ratio() - 0.5).abs() < 1e-9);
        assert_eq!(PrioritySlice { delivered: 0, injected: 0 }.ratio(), 0.0);
    }
}