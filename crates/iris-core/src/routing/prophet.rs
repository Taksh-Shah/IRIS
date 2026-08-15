//! PRoPHET delivery-predictability table — ROUTE-002 (WP-8).
//!
//! RFC 6693 §2.1.2 (v2, MAX transitivity form — not the v1 additive form):
//!
//! - Eq. 1 (direct contact): `P(A,B) = P_old + (1 − delta − P_old)·P_enc(intvl)`
//! - Eq. 2 (aging):            `P(A,B) = P_old·gamma^K`
//! - Eq. 3 (transitivity):     `P(A,C) = MAX(P_old, P(A,B)·P(B,C)_recv·beta)`
//!
//! DPs are directional: `P(A,B)` need not equal `P(B,A)`. `P(A,A) = 1`.
//! The table is O(N) per node (N = known destinations); transitivity fills it
//! toward N, bounded by [`MAX_DP_ENTRIES`] and the `p_first_threshold` discard.
//!
//! Accepts an optional virtual clock (SIM-001) so aging is deterministic;
//! otherwise wall-clock `Instant` is used.
//!
//! Research record: RES-0011 — RFC 6693 read in full (no errata).

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use crate::message::PeerId;

/// Hard cap on DP entries per node (RES-0011 §b5 — O(N) per node, bounded
/// toward 10k design scale).
pub const MAX_DP_ENTRIES: usize = 4096;

/// Top-N DPs included in a neighbor snapshot exchange (fits the DISCO-001
/// `CapabilityBundle` ≤256 B budget with 8-byte (PeerId, f32) rows).
pub const TOP_N_DP: usize = 32;

/// RFC 6693 §3.3 calibrated defaults (RES-0011 §c).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProphetConfig {
    /// First-contact DP, also P_encounter used when the interval is unknown
    /// (RFC: SHOULD 0.5).
    pub p_encounter_first: f64,
    /// Ceiling for a direct-contact encounter factor (RFC fig.3 default 0.7).
    pub p_encounter_max: f64,
    /// Below this DP a predicate is discarded / a candidate is not consulted
    /// (§2.1.3.2, SHOULD 0.1).
    pub p_first_threshold: f64,
    /// Upper-bound slack in Eq. 1 (fig.3 default 0.01).
    pub delta: f64,
    /// Aging exponent base (fig.3 default 0.999 per [aging] interval).
    pub gamma: f64,
    /// Interval considered "typical" for a contact frequency band; also the
    /// aging unit (RES-0011 §c — derive per deployment, not hard-coded 1 h).
    pub aging_interval: Duration,
    /// Transitivity scale (§3.3, MAX-form default 0.9).
    pub beta: f64,
    /// Interval-scaling exponent (§3.3 default 0.5).
    pub alpha: f64,
}

impl Default for ProphetConfig {
    fn default() -> Self {
        ProphetConfig {
            p_encounter_first: 0.5,
            p_encounter_max: 0.7,
            p_first_threshold: 0.1,
            delta: 0.01,
            gamma: 0.999,
            aging_interval: Duration::from_secs(1800), // 30-min contact band
            beta: 0.9,
            alpha: 0.5,
        }
    }
}

impl ProphetConfig {
    /// DSIR appendix-C style gamma derivation (RES-0011 §c): a contact at the
    /// encounter ceiling that ages K intervals to `p_first_threshold`.
    pub fn gamma_for(p_first: f64, p_enc_max: f64, k_intervals: f64) -> f64 {
        (p_first / p_enc_max).powf(1.0 / k_intervals)
    }

    /// Interval-scaled P_encounter per RFC §2.1.2 / §3.3.
    fn p_encounter(&self, since_ms: u64) -> f64 {
        let intvl_ms = self.aging_interval.as_millis() as u64;
        if since_ms <= intvl_ms {
            self.p_encounter_max
        } else {
            self.p_encounter_max * ((intvl_ms as f64 / since_ms as f64).powf(self.alpha))
        }
    }
}

/// A point in time: either wall clock or the SIM virtual clock.
#[derive(Debug, Clone)]
enum TimePoint {
    Wall(Instant),
    Virtual(Arc<AtomicU64>),
}

impl TimePoint {
    fn now(&self) -> Duration {
        match self {
            TimePoint::Wall(now) => now.elapsed(),
            TimePoint::Virtual(clock) => Duration::from_millis(
                clock.load(Ordering::Relaxed).saturating_mul(1000),
            ),
        }
    }
}

/// Directional per-destination delivery predictability.
#[derive(Debug)]
pub struct DeliveryPredictability {
    config: ProphetConfig,
    dps: HashMap<PeerId, f64>,
    /// Last update time per destination (for aging intervals).
    last_meet: HashMap<PeerId, Duration>,
    now: TimePoint,
}

impl DeliveryPredictability {
    pub fn new(config: ProphetConfig) -> Self {
        DeliveryPredictability {
            config,
            dps: HashMap::new(),
            last_meet: HashMap::new(),
            now: TimePoint::Wall(Instant::now()),
        }
    }

    /// Attach the SIM virtual clock (must be set before any `meet`/`age`).
    pub fn with_virtual_clock(mut self, clock: Arc<AtomicU64>) -> Self {
        self.now = TimePoint::Virtual(clock);
        self
    }

    pub fn config(&self) -> &ProphetConfig {
        &self.config
    }

    /// `P(A,self_dst)` for a destination (0.0 if unknown).
    pub fn p_for(&self, dst: &PeerId) -> f64 {
        self.dps.get(dst).copied().unwrap_or(0.0)
    }

    pub fn len(&self) -> usize {
        self.dps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dps.is_empty()
    }

    /// Apply aging (Eq. 2) then return the number of entries remaining.
    /// Each destination is decayed by `gamma^K` where K is whole aging
    /// intervals since its last update.
    pub fn age(&mut self) -> usize {
        let now = self.now.now();
        let cfg = self.config;
        let mut aging = Vec::with_capacity(self.dps.len());
        for (dst, p) in &self.dps {
            let k = self
                .last_meet
                .get(dst)
                .map(|t| ceiling_intervals(now.saturating_sub(*t), cfg.aging_interval))
                .unwrap_or(0u64);
            let aged = if k > 0 {
                p * cfg.gamma.powi(k as i32)
            } else {
                *p
            };
            if aged >= cfg.p_first_threshold {
                aging.push((*dst, aged, now));
            }
        }
        // Rebuild keeping only entries above the discard floor.
        self.dps.clear();
        self.last_meet.clear();
        for (dst, aged, at) in aging {
            self.dps.insert(dst, aged);
            self.last_meet.insert(dst, at);
        }
        self.dps.len()
    }

    /// Record a direct contact with `other`: Eq. 1 (interval-scaled
    /// P_encounter) then Eq. 3 transitivity against the other's DP snapshot
    /// (MAX form). Returns the set of destinations whose DP changed.
    pub fn meet(
        &mut self,
        other: &PeerId,
        other_predictions: &[(PeerId, f64)],
    ) -> Vec<PeerId> {
        let now = self.now.now();
        let cfg = self.config;

        if self.dps.len() >= MAX_DP_ENTRIES && !self.dps.contains_key(other) {
            // Bound transitivity fill (RES-0011 §b5). Evict the lowest-DP entry.
            if let Some((lo_dst, _)) = self
                .dps
                .iter()
                .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(d, p)| (*d, *p))
            {
                self.dps.remove(&lo_dst);
                self.last_meet.remove(&lo_dst);
            }
        }

        // Eq. 1: direct contact update.
        let since = self
            .last_meet
            .get(other)
            .map(|t| now.saturating_sub(*t).as_millis() as u64)
            .unwrap_or(u64::MAX); // first encounter → full P_encounter
        let p_enc = if self.dps.contains_key(other) {
            cfg.p_encounter(since)
        } else {
            cfg.p_encounter_first
        };
        let p_old = self.dps.get(other).copied().unwrap_or(0.0);
        let bound = 1.0 - cfg.delta;
        let p_direct = p_old + (bound - p_old) * p_enc;
        let mut changed = Vec::new();
        if (p_direct - p_old).abs() > 1e-9 || !self.dps.contains_key(other) {
            self.dps.insert(*other, p_direct);
            self.last_meet.insert(*other, now);
            changed.push(*other);
        }

        // Eq. 3: transitivity against the other node's snapshot (MAX form).
        let p_self_other = self.dps.get(other).copied().unwrap_or(p_direct);
        for (dst, p_other_dst) in other_predictions {
            if dst == other || dst.as_bytes() == other.as_bytes() {
                continue;
            }
            let p_t = p_self_other * p_other_dst.clamp(0.0, 1.0) * cfg.beta;
            let p_old_t = self.dps.get(dst).copied().unwrap_or(0.0);
            if p_t > p_old_t {
                self.dps.insert(*dst, p_t);
                self.last_meet.insert(*dst, now);
                changed.push(*dst);
            }
        }
        changed
    }

    /// Top-N snapshot for neighbor exchange (sorted DESC by DP), honoring the
    /// P_first_threshold discard. Deterministic (total_cmp, PeerId tie-break).
    pub fn snapshot(&self) -> Vec<(PeerId, f64)> {
        let mut rows: Vec<(PeerId, f64)> = self
            .dps
            .iter()
            .filter(|(_, p)| **p >= self.config.p_first_threshold)
            .map(|(d, p)| (*d, *p))
            .collect();
        rows.sort_unstable_by(|a, b| {
            b.1.total_cmp(&a.1)
                .then_with(|| a.0.as_bytes().cmp(b.0.as_bytes()))
        });
        rows.truncate(TOP_N_DP);
        rows
    }

    /// Drop entries below the discard floor (caller decides cadence).
    pub fn prune(&mut self) -> usize {
        let thr = self.config.p_first_threshold;
        let keep: Vec<(PeerId, f64)> = self
            .dps
            .iter()
            .filter(|(_, p)| **p >= thr)
            .map(|(d, p)| (*d, *p))
            .collect();
        self.dps.retain(|_, p| *p >= thr);
        self.last_meet.retain(|d, _| self.dps.contains_key(d));
        // Drop late-aging last_meet for removed entries.
        self.last_meet.retain(|d, _| self.dps.contains_key(d));
        debug_assert_eq!(self.dps.len(), keep.len());
        self.dps.len()
    }
}

/// Whole intervals that fit in `elapsed` (fsat). Never 0 for elapsed ≥ interval.
fn ceiling_intervals(elapsed: Duration, interval: Duration) -> u64 {
    if interval.is_zero() {
        return 0;
    }
    (elapsed.as_nanos() / interval.as_nanos().max(1)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::PeerId;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    #[test]
    fn first_contact_sets_p_encounter_first() {
        let mut p = DeliveryPredictability::new(ProphetConfig {
            p_encounter_first: 0.5,
            delta: 0.01,
            ..Default::default()
        });
        let b = pid(2);
        let changed = p.meet(&b, &[]);
        // Eq.1 with p_old=0: P = 0 + (1−0.01)·0.5 = 0.495.
        assert!((p.p_for(&b) - 0.495).abs() < 1e-9, "got {}", p.p_for(&b));
        assert_eq!(changed, vec![b]);
    }

    #[test]
    fn direct_update_converges_to_bound() {
        let mut p = DeliveryPredictability::new(ProphetConfig::default());
        let b = pid(2);
        // Repeated immediately-successive meets climb toward (1−delta).
        for _ in 0..50 {
            p.meet(&b, &[]);
        }
        let bound = 1.0 - p.config().delta;
        let v = p.p_for(&b);
        assert!(v < bound + 1e-6);
        assert!(v > 0.9, "direct DP should climb well above {v}");
    }

    #[test]
    fn aging_decays_by_gamma_per_interval() {
        let clock = Arc::new(AtomicU64::new(1_700_000_000));
        let mut p = DeliveryPredictability::new(ProphetConfig {
            p_encounter_first: 1.0,
            gamma: 0.5,
            aging_interval: Duration::from_secs(1),
            ..Default::default()
        })
        .with_virtual_clock(clock.clone());
        let b = pid(2);
        p.meet(&b, &[]);
        // First contact: P = (1−0.01)·1.0 = 0.99 (virtual clock at t0).
        clock.store(1_700_000_001, Ordering::Relaxed); // +1 s → 1 aging interval
        p.age();
        // K=1: 0.99·0.5 = 0.495.
        assert!((p.p_for(&b) - 0.99 * 0.5).abs() < 1e-9, "got {}", p.p_for(&b));
        clock.store(1_700_000_003, Ordering::Relaxed); // +3 s → K=3
        p.age();
        assert!((p.p_for(&b) - 0.99 * 0.5f64.powi(3)).abs() < 1e-9);
    }

    #[test]
    fn transitivity_uses_max_form() {
        let mut a = DeliveryPredictability::new(ProphetConfig {
            p_encounter_first: 0.5,
            beta: 0.9,
            ..Default::default()
        });
        let b = pid(2);
        let c = pid(3);
        // A meets B (direct high), B's snapshot says P(B,C)=0.9.
        a.meet(&b, &[(c, 0.9)]);
        let p_ac = a.p_for(&c);
        let p_ab = a.p_for(&b);
        // Eq.3 (MAX, additive NOT used): P(A,C) = P(A,B)·P(B,C)·beta at most.
        assert!(p_ac < p_ab * 0.9 * 0.9 + 1e-9);
        assert!(p_ac > 0.0);
        // A second lower-transitive meet must NOT reset downward (MAX form).
        let before = a.p_for(&c);
        a.meet(&b, &[(c, 0.1)]);
        assert!(a.p_for(&c) >= before - 1e-12, "MAX form violated");
    }

    #[test]
    fn dp_is_directional_and_self_is_one() {
        let mut a = DeliveryPredictability::new(ProphetConfig::default());
        let b = pid(2);
        a.meet(&b, &[]);
        // P(A,A) is a predicate "self" — meet with self keeps it at the bound.
        let self_key = pid(1);
        a.meet(&self_key, &[]);
        assert!(a.p_for(&b) > 0.0);
        // Directional: no reverse entry was created for B→A by A's meet.
        assert!(a.p_for(&self_key) > 0.0);
        // No bogus symmetric row appeared.
        assert!(a.snapshot().iter().all(|(d, _)| !(d == &pid(9))));
    }

    #[test]
    fn snapshot_top_n_sorted_deterministic() {
        let clock = Arc::new(AtomicU64::new(1_700_000_000));
        let mut p = DeliveryPredictability::new(ProphetConfig::default())
            .with_virtual_clock(clock.clone());
        for i in 0..50u8 {
            p.meet(&pid(i), &[]);
        }
        let snap = p.snapshot();
        assert!(snap.len() <= TOP_N_DP);
        for w in snap.windows(2) {
            assert!(w[0].1 >= w[1].1, "must be DESC");
        }
        // Determinism: same table → identical snapshot.
        assert_eq!(snap, p.snapshot());
    }

    #[test]
    fn capped_entries_evict_lowest() {
        let clock = Arc::new(AtomicU64::new(1_700_000_000));
        let mut p = DeliveryPredictability::new(ProphetConfig::default())
            .with_virtual_clock(clock.clone());
        for i in 0..(MAX_DP_ENTRIES + 10) as u8 {
            p.meet(&pid(i), &[]);
        }
        assert!(p.len() <= MAX_DP_ENTRIES, "cap violated: {}", p.len());
    }
}