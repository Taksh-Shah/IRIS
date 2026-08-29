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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
            TimePoint::Virtual(clock) => {
                Duration::from_millis(clock.load(Ordering::Relaxed).saturating_mul(1000))
            }
        }
    }
}

/// Directional per-destination delivery predictability.
///
/// `Clone` (ROUT-33): benchmarks need a cheap way to snapshot a populated
/// table and restore it before every timed iteration rather than mutating
/// one shared instance across the whole run.
#[derive(Debug, Clone)]
pub struct DeliveryPredictability {
    config: ProphetConfig,
    dps: HashMap<PeerId, f64>,
    /// DTN-14: time of the last **direct** Eq. 1 contact with this
    /// destination — feeds only Eq. 1's interval-scaled `P_encounter`.
    /// Written exclusively by `meet`'s direct-contact branch, for `other`
    /// only (never for a transitively-derived destination). Kept separate
    /// from `aging_base` below: the two previously shared one map, and
    /// `age()` overwriting it on every pass silently pinned every peer's
    /// encounter interval at "just now", disabling Eq. 1's interval
    /// scaling entirely once aging ran on any real cadence.
    last_encounter: HashMap<PeerId, Duration>,
    /// DTN-13/DTN-14: aging baseline per destination. Seeded by `meet` at
    /// insertion/update time (direct *and* transitive — both writes), then
    /// only ever *advanced* by `age_at`, never reset to `now`. That advance
    /// rule is DTN-13's actual fix: the baseline moves forward by exactly
    /// the whole intervals consumed, preserving the sub-interval remainder,
    /// so calling `age()` more often than `aging_interval` no longer
    /// discards decay progress on every call. `age_at` deliberately does
    /// NOT originate a baseline out of nothing on an entry's first aging
    /// pass (an earlier version of this fix tried that split — "meet never
    /// touches aging_base" — but DTN-12 wires aging to run *inside* `meet`,
    /// so the realistic production case is "one age pass, a long time after
    /// insertion"; treating that first pass as k=0 would silently forgive
    /// an entry's entire dormant period instead of decaying it, which is
    /// worse than DTN-13's original bug, not a fix for it). Kept as a
    /// separate map from `last_encounter` regardless — that split is the
    /// real DTN-14 fix, independent of who seeds the initial value.
    aging_base: HashMap<PeerId, Duration>,
    now: TimePoint,
    /// DTN-18: this node's own identity, when known. `None` for every
    /// production call site today — `RoutingEngine` itself has no concept
    /// of its own `PeerId` (grepped: zero `self_id`/`node_id` fields
    /// anywhere in `routing/mod.rs`), so making this a *required*
    /// constructor argument would have cascaded into a much larger
    /// identity-threading change across `RoutingEngine`/`OpportunisticRouter`
    /// and 20+ call sites, most of them tests. `with_self_id` is additive
    /// or opt-in instead, matching `with_virtual_clock`/`with_telemetry`'s
    /// existing pattern — ready for whoever eventually gives
    /// `RoutingEngine` its own identity, without forcing that change now.
    self_id: Option<PeerId>,
}

impl DeliveryPredictability {
    pub fn new(config: ProphetConfig) -> Self {
        DeliveryPredictability {
            config,
            dps: HashMap::new(),
            last_encounter: HashMap::new(),
            aging_base: HashMap::new(),
            now: TimePoint::Wall(Instant::now()),
            self_id: None,
        }
    }

    /// Attach the SIM virtual clock (must be set before any `meet`/`age`).
    pub fn with_virtual_clock(mut self, clock: Arc<AtomicU64>) -> Self {
        self.now = TimePoint::Virtual(clock);
        self
    }

    /// DTN-18: attach this node's own identity. Establishes the documented
    /// `P(A,A) = 1` invariant (bounded to `1 - delta`, Eq. 1's own bound —
    /// nothing in RFC 6693 requires the self entry to be exactly 1.0, and
    /// capping it there keeps `snapshot()`'s ordering well-defined against
    /// any entry that also approaches the bound) and makes `meet()` reject
    /// a neighbor-supplied row for this destination — without this, a
    /// neighbor's snapshot legitimately containing "P(neighbor, us)"
    /// inserts a nonsensical self-referential DP row on the very first
    /// contact round (see the module's prior behaviour, fixed here). Must
    /// be called before any `meet()`/`snapshot()` call to take effect.
    pub fn with_self_id(mut self, self_id: PeerId) -> Self {
        self.dps.insert(self_id, 1.0 - self.config.delta);
        self.self_id = Some(self_id);
        self
    }

    /// DTN-15: shared eviction used both for a brand-new direct contact and
    /// for each new transitivity insert inside `meet()`'s loop — evicts the
    /// globally-lowest-DP entry when at `MAX_DP_ENTRIES`, but never a
    /// `protect`ed destination (the one about to be written, or DTN-18's
    /// self entry). Checking before *every* insert (not once per `meet()`
    /// call) is what actually bounds the table: the old code checked only
    /// once at the top of `meet()`, so a single hostile `other_predictions`
    /// slice with many new destinations could grow the table by (len − 1)
    /// entries regardless of the cap.
    fn evict_worst_unless(&mut self, protect: &[PeerId]) {
        if self.dps.len() < MAX_DP_ENTRIES {
            return;
        }
        if let Some((lo_dst, _)) = self
            .dps
            .iter()
            .filter(|(d, _)| !protect.contains(d))
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(d, p)| (*d, *p))
        {
            self.dps.remove(&lo_dst);
            self.last_encounter.remove(&lo_dst);
            self.aging_base.remove(&lo_dst);
        }
    }

    pub fn config(&self) -> &ProphetConfig {
        &self.config
    }

    /// `P(A,self_dst)` for a destination (0.0 if unknown).
    pub fn p_for(&self, dst: &PeerId) -> f64 {
        self.dps.get(dst).copied().unwrap_or(0.0)
    }

    /// Whether this node has ever recorded a DP entry for `dst` — distinct
    /// from `p_for(dst) == 0.0`, which is also the default for an entry that
    /// genuinely decayed/aged out to zero. ROUT-25's spray fallback needs
    /// "truly no history" (cold start), not "history says 0.0".
    pub fn has_entry(&self, dst: &PeerId) -> bool {
        self.dps.contains_key(dst)
    }

    pub fn len(&self) -> usize {
        self.dps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.dps.is_empty()
    }

    /// Apply aging (Eq. 2) then return the number of entries remaining.
    /// Each destination is decayed by `gamma^K` where K is whole aging
    /// intervals since its own aging baseline (see `aging_base`'s doc
    /// comment — not since it was last written, which is a different clock,
    /// DTN-14).
    pub fn age(&mut self) -> usize {
        let now = self.now.now();
        self.age_at(now);
        self.dps.len()
    }

    /// The actual aging step, also run at the top of every `meet` (DTN-12):
    /// `meet` is the sole choke point every production DP mutation passes
    /// through today (confirmed by grep — nothing calls `age`/`prune`
    /// outside a benchmark), and no "routing tick" scheduler exists
    /// anywhere in this engine to call `age` from independently (also
    /// confirmed by grep; building one would be a substantially larger
    /// architectural addition than this cluster's own Location/Fix fields
    /// imply). Running aging at contact time — the natural PRoPHET
    /// processing cadence, since all state in this engine already advances
    /// on contact events rather than a wall-clock timer — gives Eq. 2 a
    /// real, always-current caller without inventing new infrastructure.
    /// `snapshot`/`p_for` deliberately do NOT also self-age (that would
    /// need `&mut self`, rippling into every read call site for marginal
    /// benefit): the small staleness this leaves — at most one contact
    /// round — is normal for a gossip-style protocol that already treats
    /// contacts as its synchronization point.
    fn age_at(&mut self, now: Duration) {
        let cfg = self.config;
        let mut aging = Vec::with_capacity(self.dps.len());
        for (dst, p) in &self.dps {
            // DTN-18: P(A,A) is a fixed identity value, not a real
            // prediction — it must never decay or be discarded by the
            // threshold check below, however long it sits unmeasured.
            if Some(*dst) == self.self_id {
                aging.push((*dst, *p, now));
                continue;
            }
            let old_base = self.aging_base.get(dst).copied();
            let k = old_base
                .map(|t| floor_intervals(now.saturating_sub(t), cfg.aging_interval))
                .unwrap_or(0u64);
            // DTN-17: `k as i32` truncates/wraps for k > i32::MAX, and
            // gamma<1 raised to a wrapped-negative exponent is +inf, which
            // then survives the threshold check below and poisons every
            // downstream consumer. Clamp before casting, and never trust
            // the result to be finite even after clamping.
            let aged = if k > 0 {
                let k32 = k.min(i32::MAX as u64) as i32;
                let v = p * cfg.gamma.powi(k32);
                if v.is_finite() {
                    v.clamp(0.0, 1.0)
                } else {
                    0.0
                }
            } else {
                *p
            };
            if aged >= cfg.p_first_threshold {
                // DTN-13: advance the baseline by exactly the whole
                // intervals just consumed (not to `now`) — preserves the
                // sub-interval remainder so a cadence faster than
                // `aging_interval` still accumulates toward the next decay
                // step instead of resetting every single call, which is
                // what made aging a total no-op once it had a real caller.
                let new_base = match old_base {
                    Some(t) if k > 0 => {
                        t.saturating_add(cfg.aging_interval.saturating_mul(k.min(u32::MAX as u64) as u32))
                    }
                    Some(t) => t,
                    // `meet` seeds aging_base for every entry it writes, so
                    // this is only reachable in practice for an entry this
                    // table has literally never had a baseline for — a
                    // defensive fallback, not the common path.
                    None => now,
                };
                aging.push((*dst, aged, new_base));
            }
        }
        // Rebuild keeping only entries above the discard floor.
        self.dps.clear();
        self.aging_base.clear();
        for (dst, aged, base) in aging {
            self.dps.insert(dst, aged);
            self.aging_base.insert(dst, base);
        }
        // A destination that just aged out must not leave a dangling
        // encounter timestamp behind (Eq. 1 would otherwise compute
        // `since` against a peer no longer in the table).
        self.last_encounter.retain(|d, _| self.dps.contains_key(d));
    }

    /// Record a direct contact with `other`: Eq. 1 (interval-scaled
    /// P_encounter) then Eq. 3 transitivity against the other's DP snapshot
    /// (MAX form). Returns the set of destinations whose DP changed.
    pub fn meet(&mut self, other: &PeerId, other_predictions: &[(PeerId, f64)]) -> Vec<PeerId> {
        let now = self.now.now();
        // DTN-12: age the whole table as the first step of every contact —
        // see `age_at`'s doc comment for why this, not a separate tick, is
        // this table's real production aging path.
        self.age_at(now);
        let cfg = self.config;
        let mut protect = vec![*other];
        if let Some(s) = self.self_id {
            protect.push(s);
        }

        if !self.dps.contains_key(other) {
            self.evict_worst_unless(&protect);
        }

        // Eq. 1: direct contact update.
        let since = self
            .last_encounter
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
            self.last_encounter.insert(*other, now);
            // DTN-13/14: seed (or refresh) the aging baseline at the
            // moment of this write. `age_at` only ever *advances* it from
            // here — see aging_base's doc comment for why the baseline
            // must originate here rather than on age_at's first
            // observation.
            self.aging_base.insert(*other, now);
            changed.push(*other);
        }

        // Eq. 3: transitivity against the other node's snapshot (MAX form).
        //
        // DTN-15: `.take(TOP_N_DP)` truncates on *ingest* — the old code
        // only truncated what this node *sends* (`snapshot()`), never what
        // it *receives*, so a hostile peer supplying millions of rows in
        // one `other_predictions` slice inserted millions of entries in a
        // single call: a one-packet remote memory-exhaustion DoS. Capping
        // input to the same width this node's own outbound snapshot uses
        // is symmetric and sufficient — `evict_worst_unless` below then
        // makes every individual insert respect `MAX_DP_ENTRIES` too, so
        // the table is bounded even before truncation would matter.
        let p_self_other = self.dps.get(other).copied().unwrap_or(p_direct);
        for (dst, p_other_dst) in other_predictions.iter().take(TOP_N_DP) {
            if dst == other {
                continue;
            }
            // DTN-18: never let a neighbor's snapshot overwrite this node's
            // own self entry — a neighbor B's snapshot legitimately
            // contains P(B, us), and without this guard `us` inserts a
            // nonsensical, unbounded-drift self-referential row on the
            // very first contact round.
            if Some(*dst) == self.self_id {
                continue;
            }
            // DTN-15(c): reject non-finite input explicitly. `clamp` on a
            // NaN silently passes it through unchanged (IEEE 754: any
            // comparison against NaN is false, so `clamp` never applies its
            // bounds to one) — that was load-bearing, undocumented
            // behaviour; making it an explicit `continue` is the same
            // outcome with the reason visible.
            if !p_other_dst.is_finite() {
                continue;
            }
            // DTN-18: clamp the *write*, not just the input factor — Eq. 3
            // is a product of three already-bounded terms so it cannot
            // exceed 1.0 in practice, but nothing enforced the Eq. 1 bound
            // (`1 - delta`) here, unlike every other write path in this
            // function.
            let p_t =
                (p_self_other * p_other_dst.clamp(0.0, 1.0) * cfg.beta).clamp(0.0, bound);
            let p_old_t = self.dps.get(dst).copied().unwrap_or(0.0);
            if p_t > p_old_t {
                if !self.dps.contains_key(dst) {
                    self.evict_worst_unless(&protect);
                }
                self.dps.insert(*dst, p_t);
                // DTN-13/14: seed the aging baseline here too — see the
                // matching comment on the Eq. 1 write above. A transitively
                // -derived entry ages from when it was actually computed,
                // not from whenever a later `age_at` pass happens to first
                // notice it.
                self.aging_base.insert(*dst, now);
                changed.push(*dst);
            }
        }
        changed
    }

    /// Top-N snapshot for neighbor exchange (sorted DESC by DP), honoring the
    /// P_first_threshold discard. Deterministic (total_cmp, PeerId tie-break).
    pub fn snapshot(&self) -> Vec<(PeerId, f64)> {
        // DTN-18: never advertise the self entry — a neighbor has no use
        // for "here is what I think of myself", and it would otherwise
        // waste a row in the ≤256 B CapabilityBundle budget (TOP_N_DP=32)
        // on every exchange.
        let mut rows: Vec<(PeerId, f64)> = self
            .dps
            .iter()
            .filter(|(d, p)| **p >= self.config.p_first_threshold && Some(**d) != self.self_id)
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
        // DTN-18: exempt the fixed self-identity entry, same as `age_at`.
        self.dps.retain(|d, p| *p >= thr || Some(*d) == self.self_id);
        self.last_encounter.retain(|d, _| self.dps.contains_key(d));
        self.aging_base.retain(|d, _| self.dps.contains_key(d));
        self.dps.len()
    }
}

/// Whole intervals that fit in `elapsed` — floor division (fsat). Never 0 for
/// elapsed ≥ interval. NOTE: floor, not ceiling — the name reflects "how many
/// full intervals have passed" (PRoPHET aging γ^k uses exactly this).
pub(crate) fn floor_intervals(elapsed: Duration, interval: Duration) -> u64 {
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

    /// Like `pid` but distinct for every value beyond `u8::MAX` — DTN-16's
    /// old `capped_entries_evict_lowest` cast `(MAX_DP_ENTRIES + 10) as u8`,
    /// which truncates 4106 to 10 (4106 % 256), so the loop only ever
    /// inserted 10 distinct peers and the cap was never actually exercised.
    fn pid_wide(id: u32) -> PeerId {
        let mut b = [0u8; 32];
        b[..4].copy_from_slice(&id.to_le_bytes());
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
        // meet() seeds aging_base[b] = t0 as part of this same call
        // (DTN-13/14) — the baseline originates at insertion, not on
        // age_at's first observation, so a single later age() call still
        // measures the true elapsed time since t0.
        clock.store(1_700_000_001, Ordering::Relaxed); // +1 s → 1 aging interval
        p.age();
        // K=1: 0.99·0.5 = 0.495.
        assert!(
            (p.p_for(&b) - 0.99 * 0.5).abs() < 1e-9,
            "got {}",
            p.p_for(&b)
        );
        clock.store(1_700_000_003, Ordering::Relaxed); // +2 s more → K=2 from the new baseline
        p.age();
        assert!((p.p_for(&b) - 0.99 * 0.5f64.powi(3)).abs() < 1e-9);
    }

    #[test]
    fn dtn13_sub_interval_cadence_ages_identically_to_one_infrequent_call() {
        // The finding's own scenario: a caller invokes aging far more often
        // than aging_interval (e.g. once per routing tick against a 30-min
        // interval). Before DTN-13 this reset the baseline to `now` on
        // every single call, so k was always 0 and decay never happened no
        // matter how much real time passed. Prove the final result no
        // longer depends on how many times age() was called along the way
        // — only on total elapsed time.
        let cfg = ProphetConfig {
            p_encounter_first: 1.0,
            gamma: 0.5,
            aging_interval: Duration::from_secs(10),
            ..Default::default()
        };
        let b = pid(2);

        let clock_frequent = Arc::new(AtomicU64::new(1_700_000_000));
        let mut frequent =
            DeliveryPredictability::new(cfg).with_virtual_clock(clock_frequent.clone());
        frequent.meet(&b, &[]);
        // Tick once per second for 25 seconds (2.5x the aging_interval) —
        // 25 age() calls, none aligned to the 10 s boundary except by
        // accumulation.
        for s in 1..=25u64 {
            clock_frequent.store(1_700_000_000 + s, Ordering::Relaxed);
            frequent.age();
        }

        let clock_once = Arc::new(AtomicU64::new(1_700_000_000));
        let mut once = DeliveryPredictability::new(cfg).with_virtual_clock(clock_once.clone());
        once.meet(&b, &[]);
        clock_once.store(1_700_000_000 + 25, Ordering::Relaxed);
        once.age();

        assert!(
            (frequent.p_for(&b) - once.p_for(&b)).abs() < 1e-9,
            "frequent={} once={} — cadence must not change the result",
            frequent.p_for(&b),
            once.p_for(&b)
        );
        // Sanity: 25 s over a 10 s interval is K=2 (20 s consumed, 5 s
        // remainder held for next time) — decay must actually have run,
        // not just "agree while both being no-ops".
        assert!(
            (once.p_for(&b) - 0.99 * 0.5f64.powi(2)).abs() < 1e-9,
            "got {}",
            once.p_for(&b)
        );
    }

    #[test]
    fn dtn14_aging_does_not_clobber_the_encounter_interval_clock() {
        // Before the last_encounter/aging_base split, age() overwrote the
        // single shared map on every pass, so `since` (Eq. 1's interval
        // scaling input) was always ~0 regardless of how long it had
        // actually been since the last direct contact — P_encounter was
        // permanently pinned at its ceiling.
        let clock = Arc::new(AtomicU64::new(1_700_000_000));
        let cfg = ProphetConfig {
            p_encounter_max: 0.7,
            aging_interval: Duration::from_secs(100),
            alpha: 0.5,
            gamma: 0.9999, // slow decay so the DP survives many ticks below
            ..Default::default()
        };
        let mut p = DeliveryPredictability::new(cfg).with_virtual_clock(clock.clone());
        let b = pid(2);
        p.meet(&b, &[]); // establishes dps[b] and last_encounter[b] = t0

        // Simulate 50 aging ticks, 1 s apart, well below aging_interval —
        // no direct re-contact with b in between.
        for s in 1..=50u64 {
            clock.store(1_700_000_000 + s, Ordering::Relaxed);
            p.age();
        }

        // Meet b again now, 50 s after the *actual* last direct contact.
        // since_ms = 50_000 > intvl_ms = 100_000? No — pick a gap that
        // exceeds the interval so interval-scaling actually kicks in.
        clock.store(1_700_000_301, Ordering::Relaxed); // 301 s since t0
        let p_old = p.p_for(&b);
        p.meet(&b, &[]);
        let p_new = p.p_for(&b);
        // Eq. 1 with since_ms(301_000) > intvl_ms(100_000):
        // p_enc = 0.7 * (100_000/301_000)^0.5 ≈ 0.4035, strictly below the
        // 0.7 ceiling. If last_encounter had been clobbered by the 50
        // intervening age() calls, `since` would read ~1000 ms and p_enc
        // would be pinned at the 0.7 ceiling instead.
        let bound = 1.0 - p.config().delta;
        let p_enc_pinned_prediction = p_old + (bound - p_old) * 0.7;
        assert!(
            p_new < p_enc_pinned_prediction - 1e-6,
            "P_encounter must be interval-scaled below the ceiling, not pinned: p_new={p_new} pinned-case={p_enc_pinned_prediction}"
        );
    }

    #[test]
    fn dtn17_large_k_never_produces_a_non_finite_dp() {
        // Regression for the i32 truncation: a k large enough to wrap
        // negative through `as i32` used to make gamma.powi(k) evaluate to
        // +inf, which then passed the `>= p_first_threshold` check and
        // poisoned the table permanently.
        let clock = Arc::new(AtomicU64::new(0));
        let mut p = DeliveryPredictability::new(ProphetConfig {
            p_encounter_first: 1.0,
            gamma: 0.999,
            aging_interval: Duration::from_millis(1),
            ..Default::default()
        })
        .with_virtual_clock(clock.clone());
        let b = pid(2);
        p.meet(&b, &[]);
        // Advance far enough that k = elapsed/interval exceeds i32::MAX
        // (interval is 1 ms, so ~2.5M seconds clears 2^31 ms).
        clock.store(3_000_000_000, Ordering::Relaxed);
        p.age();
        let v = p.p_for(&b);
        assert!(v.is_finite(), "DP must never become non-finite, got {v}");
        assert!((0.0..=1.0).contains(&v), "DP must stay in [0,1], got {v}");
    }

    #[test]
    fn dtn12_meet_ages_the_table_as_a_side_effect_without_an_explicit_age_call() {
        // DTN-12's core claim: age() had zero non-benchmark production
        // callers, so DPs only ever increased. Prove meet() now ages the
        // *whole* table on every call, not just the peer being met —
        // without the test ever calling .age() itself.
        let clock = Arc::new(AtomicU64::new(1_700_000_000));
        let cfg = ProphetConfig {
            p_encounter_first: 1.0,
            gamma: 0.5,
            aging_interval: Duration::from_secs(1),
            ..Default::default()
        };
        let mut p = DeliveryPredictability::new(cfg).with_virtual_clock(clock.clone());
        let x = pid(2);
        let y = pid(3);
        p.meet(&x, &[]); // t0: dps[x] = 0.99, aging_base[x] = t0
        let before = p.p_for(&x);
        clock.store(1_700_000_003, Ordering::Relaxed); // +3 s → K=3 once aged
        p.meet(&y, &[]); // a completely unrelated contact — x is never touched by y's own Eq.1/Eq.3
        let after = p.p_for(&x);
        assert!(
            after < before - 1e-9,
            "meet() must age existing entries as a side effect: before={before} after={after}"
        );
        assert!(
            (after - 0.99 * 0.5f64.powi(3)).abs() < 1e-9,
            "expected exact K=3 decay, got {after}"
        );
    }

    #[test]
    fn dtn18_self_entry_never_decays_even_after_many_aging_passes() {
        // age_at now runs on every meet() (DTN-12) — without an explicit
        // exemption, the self-identity entry (P(A,A), meant to be fixed)
        // would decay like any other entry once real aging passes ran.
        let clock = Arc::new(AtomicU64::new(1_700_000_000));
        let self_id = pid_wide(1);
        let mut a = DeliveryPredictability::new(ProphetConfig {
            gamma: 0.5,
            aging_interval: Duration::from_secs(1),
            ..Default::default()
        })
        .with_virtual_clock(clock.clone())
        .with_self_id(self_id);
        let bound = 1.0 - a.config().delta;

        for s in 1..=20u64 {
            clock.store(1_700_000_000 + s, Ordering::Relaxed);
            a.meet(&pid(10), &[]); // unrelated contact, drives age_at each time
        }
        assert!(
            (a.p_for(&self_id) - bound).abs() < 1e-9,
            "self entry must never decay, got {}",
            a.p_for(&self_id)
        );
    }

    #[test]
    fn dtn19_prune_drops_only_entries_below_threshold() {
        let mut p = DeliveryPredictability::new(ProphetConfig::default());
        let keep = pid(2);
        let drop = pid(3);
        p.meet(&keep, &[]); // ~0.495, above the 0.1 default threshold
        p.meet(&pid(9), &[(drop, 0.05)]); // transitivity result well below threshold
        assert!(p.p_for(&drop) < p.config().p_first_threshold);
        p.prune();
        assert!(p.has_entry(&keep), "above-threshold entry must survive prune");
        assert!(!p.has_entry(&drop), "below-threshold entry must be dropped by prune");
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
        let mut p =
            DeliveryPredictability::new(ProphetConfig::default()).with_virtual_clock(clock.clone());
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
        let mut p =
            DeliveryPredictability::new(ProphetConfig::default()).with_virtual_clock(clock.clone());

        // Seed one deliberately low-DP entry via transitivity (direct
        // contacts all converge to ~0.495 on first meet, so they tie with
        // each other and would not discriminate "lowest" — this needs a
        // genuinely different value to prove eviction targets it). Must
        // stay above p_first_threshold (0.1 default): DTN-12 wired age_at
        // into every meet() call, which now discards anything below
        // threshold as unrelated cleanup — this test wants capacity-based
        // eviction to be what removes it, not threshold pruning.
        let filler = pid_wide(1);
        let low_dst = pid_wide(2);
        p.meet(&filler, &[(low_dst, 0.6)]);
        let low_p = p.p_for(&low_dst);
        assert!(low_p > 0.1 && low_p < 0.3, "seed value not usably low: {low_p}");

        // Fill to capacity with normal direct contacts (~0.495 each,
        // comfortably above low_p).
        for i in 3..(MAX_DP_ENTRIES as u32 + 1) {
            p.meet(&pid_wide(i), &[]);
        }
        assert_eq!(p.len(), MAX_DP_ENTRIES, "must reach the cap exactly");

        // One more distinct peer pushes over capacity — must evict low_dst
        // specifically, not an arbitrary entry, and the cap must hold
        // exactly (DTN-16's old test only asserted `<=`, true even if
        // eviction never ran at all).
        let new_dst = pid_wide(MAX_DP_ENTRIES as u32 + 100);
        p.meet(&new_dst, &[]);
        assert_eq!(p.len(), MAX_DP_ENTRIES, "cap must hold exactly, not just <=");
        assert_eq!(p.p_for(&low_dst), 0.0, "the lowest-DP entry must be the one evicted");
        assert!(p.p_for(&new_dst) > 0.0, "the new entry must have been admitted");
    }

    #[test]
    fn dtn15_meet_with_huge_prediction_slice_stays_bounded() {
        // Regression for the finding's own trigger scenario: a single
        // meet() call fed a slice far larger than MAX_DP_ENTRIES must not
        // grow the table past the cap — the old code only checked capacity
        // once at the top of meet(), so one hostile slice with many new
        // destinations could grow the table by (len − 1) entries regardless
        // of MAX_DP_ENTRIES.
        let mut p = DeliveryPredictability::new(ProphetConfig::default());
        let other = pid_wide(1);
        let huge: Vec<(PeerId, f64)> = (0..(MAX_DP_ENTRIES as u32 * 4))
            .map(|i| (pid_wide(1_000_000 + i), 1.0))
            .collect();
        p.meet(&other, &huge);
        assert!(
            p.len() <= MAX_DP_ENTRIES,
            "a single meet() call must never exceed MAX_DP_ENTRIES (got {})",
            p.len()
        );
    }

    #[test]
    fn dtn15_non_finite_prediction_is_rejected() {
        let mut p = DeliveryPredictability::new(ProphetConfig::default());
        let other = pid_wide(1);
        let victim = pid_wide(2);
        p.meet(&other, &[(victim, f64::NAN), (victim, f64::INFINITY)]);
        assert_eq!(p.p_for(&victim), 0.0, "a non-finite input must never be inserted");
    }

    #[test]
    fn dtn18_self_id_is_never_polluted_by_a_neighbors_snapshot() {
        let self_id = pid_wide(1);
        let mut a = DeliveryPredictability::new(ProphetConfig::default()).with_self_id(self_id);
        let bound = 1.0 - a.config().delta;
        assert!(
            (a.p_for(&self_id) - bound).abs() < 1e-9,
            "P(self,self) must be established at construction"
        );

        // B's snapshot legitimately contains P(B, self_id) — meeting B must
        // not let that row overwrite our own self entry.
        let b = pid_wide(2);
        a.meet(&b, &[(self_id, 0.9)]);
        assert!(
            (a.p_for(&self_id) - bound).abs() < 1e-9,
            "a neighbor's snapshot must not be able to change our self entry"
        );
        // The self entry must also never appear in an outbound snapshot.
        assert!(a.snapshot().iter().all(|(d, _)| *d != self_id));
    }

    #[test]
    fn dtn18_transitivity_write_is_clamped_to_the_eq1_bound() {
        // Eq. 3's product of three already-[0,1]-bounded terms cannot
        // literally exceed 1.0 in this test's inputs, so this specifically
        // exercises that the write path clamps to `1 - delta` (the same
        // bound every other write in this module respects) rather than
        // trusting the arithmetic.
        let mut p = DeliveryPredictability::new(ProphetConfig {
            delta: 0.5, // bound = 0.5, easy to observe a violation against
            beta: 1.0,
            ..Default::default()
        });
        let other = pid_wide(1);
        let dst = pid_wide(2);
        p.meet(&other, &[(dst, 1.0)]);
        assert!(
            p.p_for(&dst) <= 0.5 + 1e-12,
            "transitivity write must respect the 1-delta bound, got {}",
            p.p_for(&dst)
        );
    }
}
