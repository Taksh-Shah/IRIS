//! Offline-trained linear predictors for the L3 shadow experiment (ML-001).
//!
//! Both predictors are pure-Rust, no external ML runtime:
//! - [`LinearPredictor`]: logistic-style scorer over [`FeatureVec`] used for
//!   the delivery-probability prediction (LP) arm.
//! - [`GtPredictor`]: gateway-egress *timing* predictor (GT) — estimates the
//!   period of a gateway node's contact schedule from the train slice and
//!   predicts the next contact time.

use crate::sim::ml::features::FeatureVec;

/// Default training epochs (bounded, cheap — the LP arm is a baseline, not a
/// deep model).
const DEFAULT_EPOCHS: usize = 200;
/// Gradient-descent learning rate for the LP fit.
const DEFAULT_LR: f32 = 0.05;

/// Logistic-style linear predictor: `σ(w·x + b)`.
#[derive(Debug, Clone, PartialEq)]
pub struct LinearPredictor {
    pub w: [f32; 8],
    pub bias: f32,
}

impl Default for LinearPredictor {
    fn default() -> Self {
        LinearPredictor {
            w: [0.0; 8],
            bias: 0.0,
        }
    }
}

impl LinearPredictor {
    /// Score in (0,1): predicted delivery probability for a feature vector.
    pub fn score(&self, fx: &FeatureVec) -> f32 {
        fx.score(&self.w, self.bias)
    }

    /// Offline fit by gradient descent over (feature, delivered) samples.
    /// `delivered` is the supervision label ("delivered within TTL"). Runs on
    /// the train slice only; the returned predictor must be frozen before
    /// test-window shadow decisions (ML-001 §6 leakage guard).
    pub fn fit(samples: &[(FeatureVec, bool)]) -> Self {
        let mut p = LinearPredictor::default();
        if samples.is_empty() {
            return p;
        }
        for _ in 0..DEFAULT_EPOCHS {
            for (fx, y) in samples {
                let pred = p.score(fx);
                let err = pred - (*y as u32 as f32);
                let lr = DEFAULT_LR / samples.len() as f32;
                for i in 0..8 {
                    p.w[i] -= lr * err * fx.0[i];
                }
                p.bias -= lr * err;
            }
        }
        p
    }
}

/// Gateway-egress timing predictor: fits the inter-contact period of a
/// gateway-capable node and predicts the next contact time.
#[derive(Debug, Clone, PartialEq)]
pub struct GtPredictor {
    /// Estimated contact period in sim-ms (mean inter-contact interval).
    pub period_ms: f64,
}

impl Default for GtPredictor {
    fn default() -> Self {
        GtPredictor { period_ms: 0.0 }
    }
}

impl GtPredictor {
    /// Upper bound on a learnable contact period (TAK-22): one year of
    /// sim-ms. Sparse schedules beyond this are indistinguishable from "no
    /// periodic contact" for prediction purposes, and an unbounded median can
    /// otherwise overflow `predict_next`'s addition.
    pub const MAX_PERIOD_MS: f64 = 365.0 * 24.0 * 3600.0 * 1000.0;

    /// Fit the period from the train slice's contact times (sorted asc).
    /// Returns a default predictor when fewer than 2 samples (no period
    /// learnable). Input is defensively sorted (ML-RT-03).
    pub fn fit(contact_times_ms: &[u64]) -> Self {
        if contact_times_ms.len() < 2 {
            return GtPredictor::default();
        }
        let mut times: Vec<u64> = contact_times_ms.to_vec();
        times.sort_unstable();
        times.dedup(); // SIM-16: duplicate timestamps produce zero intervals that skew the median
        let mut intervals: Vec<f64> = times.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
        // Median interval — robust to one-off gaps (vehicle + stations).
        intervals.sort_by(|a, b| a.total_cmp(b));
        let mid = intervals.len() / 2;
        let period = if intervals.len() % 2 == 1 {
            intervals[mid]
        } else {
            (intervals[mid - 1] + intervals[mid]) / 2.0
        };
        GtPredictor {
            period_ms: period.clamp(1.0, Self::MAX_PERIOD_MS),
        }
    }

    /// Predicted next contact time after `last_contact_ms`.
    ///
    /// Total by construction (TAK-22): `period_ms` was clamped at fit time,
    /// and the add saturates instead of overflowing (`u64` panic in debug /
    /// silent wrap-to-tiny inversion in release on far-future contacts).
    pub fn predict_next(&self, last_contact_ms: u64) -> u64 {
        last_contact_ms.saturating_add(self.period_ms.round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(v: f32) -> FeatureVec {
        FeatureVec::new([v; 8])
    }

    #[test]
    fn fit_moves_toward_labels() {
        // Strongly separable: delivered => feature high, not => low.
        let samples = vec![
            (fx(1.0), true),
            (fx(0.9), true),
            (fx(0.1), false),
            (fx(0.0), false),
        ];
        let p = LinearPredictor::fit(&samples);
        assert!(
            p.score(&fx(1.0)) > 0.5,
            "high-feature samples should score >0.5"
        );
        assert!(
            p.score(&fx(0.0)) < 0.5,
            "low-feature samples should score <0.5"
        );
    }

    #[test]
    fn fit_empty_is_default() {
        let p = LinearPredictor::fit(&[]);
        assert_eq!(p, LinearPredictor::default());
    }

    #[test]
    fn gt_fits_periodic_schedule() {
        let times: Vec<u64> = (0..10).map(|i| 1000 * i + 50).collect();
        let g = GtPredictor::fit(&times);
        assert!((g.period_ms - 1000.0).abs() < 1.0, "period {g:?}");
        assert_eq!(g.predict_next(2050), 3050);
    }

    #[test]
    fn gt_median_resists_outlier() {
        let mut times: Vec<u64> = (0..10).map(|i| 1000 * i).collect();
        times.push(50_000); // one long gap
        let g = GtPredictor::fit(&times);
        assert!(
            (g.period_ms - 1000.0).abs() < 1.0,
            "median must ignore outlier: {g:?}"
        );
    }

    #[test]
    fn gt_short_schedule_is_default() {
        assert_eq!(GtPredictor::fit(&[100]), GtPredictor::default());
    }

    /// TAK-22: a sparse schedule with an enormous median interval must clamp
    /// at fit time, and predict_next must saturate instead of overflowing
    /// (debug panic / release wrap-inversion).
    #[test]
    fn predict_next_saturates_on_far_future_contacts() {
        let huge = u64::MAX as f64;
        let g = GtPredictor { period_ms: huge };
        // Would have been `u64::MAX - 5 + period` overflow before the fix.
        assert_eq!(g.predict_next(u64::MAX - 5), u64::MAX);
        assert_eq!(g.predict_next(0), huge.round() as u64);

        // fit-time clamp: a one-off 10-year gap cannot become the period.
        let mut times: Vec<u64> = (0..10).map(|i| 1000 * i).collect();
        times.push(times[9] + GtPredictor::MAX_PERIOD_MS as u64 * 10);
        let fitted = GtPredictor::fit(&times);
        assert!(
            fitted.period_ms <= GtPredictor::MAX_PERIOD_MS,
            "period must be clamped: {}",
            fitted.period_ms
        );
        assert_eq!(fitted.predict_next(u64::MAX), u64::MAX);
    }
}
