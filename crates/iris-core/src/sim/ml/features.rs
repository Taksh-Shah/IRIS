//! Feature extraction for the L3 shadow predictor (ML-001).
//!
//! [`FeatureVec`] is a fixed 8-slot vector built from **in-band state only**:
//! everything already produced by the L2 router, SCF buffer, hop accounting,
//! and the virtual clock. No new mutable state is added to the forwarding
//! path, and the vector is a pure function of node state → deterministic per
//! seed (ML-001 AC-1).

/// Fixed feature vector for one candidate relay decision (ML-001 §3.1).
///
/// Slot semantics (as scored for handing a message for recipient `r` to
/// neighbor `b` while at node `self`):
/// 0. `P(b,r)` — neighbor's PRoPHET delivery predictability for the recipient.
/// 1. `P(self,r)` — my delivery predictability for the recipient.
/// 2. contact frequency proxy: `P(b,self)` — neighbor's DP toward me (direct
///    encounter recency; higher = more frequent recent contacts).
/// 3. hop count of the message so far (u8 as f32).
/// 4. buffer occupancy of `b` (used bytes / cap, 0..1).
/// 5. message age (sim-seconds since timestamp).
/// 6. TTL remaining fraction (remaining / original, 0..1).
/// 7. priority rank (0..7, lower = higher priority, P0=0).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureVec(pub [f32; 8]);

impl FeatureVec {
    pub const LEN: usize = 8;

    /// Total constructor (TAK-21): non-finite slots are sanitised to `0.0`
    /// instead of being merely `debug_assert`ed away. Several documented slots
    /// are ratios whose denominators can be zero or network-influenced
    /// (`ttl_seconds == 0`, zero-capacity buffers), and in release builds the
    /// old assert compiled out entirely — letting NaN flow through `score`,
    /// into the predictor's comparator (see TAK-8), and panic inside
    /// `sort_by`. Sanitising keeps the vector finite for every input while
    /// staying deterministic per seed (ML-001 AC-1).
    pub fn new(features: [f32; 8]) -> Self {
        let mut f = features;
        for x in f.iter_mut() {
            if !x.is_finite() {
                *x = 0.0;
            }
        }
        FeatureVec(f)
    }

    /// Bounded sigmoid-affine score used by the shadow predictor. Pure
    /// arithmetic, no allocation.
    pub fn score(&self, w: &[f32; 8], bias: f32) -> f32 {
        let z = w
            .iter()
            .zip(self.0.iter())
            .fold(bias, |acc, (wi, xi)| acc + wi * xi);
        1.0 / (1.0 + (-z).exp())
    }

    /// Deterministic same-seed invariant: two feature vectors built at the
    /// same decision point must be identical (used by AC-1 tests).
    pub fn as_slice(&self) -> &[f32; 8] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_is_bounded_between_zero_and_one() {
        let f = FeatureVec::new([1.0, 0.5, 0.2, 3.0, 0.4, 120.0, 0.8, 4.0]);
        let w = [0.5; 8];
        let s = f.score(&w, 0.1);
        assert!((0.0..=1.0).contains(&s), "score must be in (0,1): {s}");
    }

    #[test]
    fn score_is_monotonic_in_positive_weight() {
        let f = FeatureVec::new([1.0, 0.5, 0.2, 3.0, 0.4, 120.0, 0.8, 4.0]);
        let w_pos = [0.5; 8];
        let w_neg = [-0.5; 8];
        assert!(f.score(&w_pos, 0.0) > f.score(&w_neg, 0.0));
    }

    /// TAK-21: NaN/inf inputs (e.g. `ttl_remaining = x / 0`) must sanitise to
    /// a finite vector and a bounded finite score in ALL build profiles — the
    /// old `debug_assert!` compiled away exactly where it mattered.
    #[test]
    fn non_finite_slots_are_sanitised_not_propagated() {
        let raw: [f32; 8] = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            3.0,
            0.4,
            120.0,
            0.8,
            4.0,
        ];
        let f = FeatureVec::new(raw);
        assert_eq!(f.0[0], 0.0);
        assert_eq!(f.0[1], 0.0);
        assert_eq!(f.0[2], 0.0);
        assert_eq!(f.0[3], 3.0, "finite slots pass through untouched");
        let s = f.score(&[0.5; 8], 0.1);
        assert!(s.is_finite(), "score must stay finite: {s}");
        assert!((0.0..=1.0).contains(&s), "score must stay bounded: {s}");
    }
}
