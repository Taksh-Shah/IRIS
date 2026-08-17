//! Shadow experiment runner — ML-001.
//!
//! Evaluates the L3 predictors against the L2 decisions actually taken, with
//! a **temporal train/test split** (first half of the timeline = train, second
//! half = test) to prevent leakage (ML-001 §6):
//! - **LP arm** ([`run_shadow_experiment`]): fits [`LinearPredictor`] on the
//!   train-slice shadow decisions (label = "message delivered within TTL"),
//!   calibrates a decision threshold on train, then reports decision-agreement
//!   with L2 GTMX+ and Brier score on the test slice.
//! - **GT arm** ([`evaluate_gt`]): fits [`GtPredictor`] on the gateway node's
//!   train-slice contact times and compares timing error against a
//!   non-robust static baseline (mean inter-contact interval — GW-001 static
//!   weights carry no timing model). Robust median should beat the mean only
//!   when contacts are periodic (ML-001 §3.2, AC-4).

use std::collections::HashSet;

use crate::protocol::MessageId;
use crate::sim::ml::features::FeatureVec;
use crate::sim::ml::predictor::GtPredictor;
use crate::sim::ml::predictor::LinearPredictor;
use crate::sim::ml::ShadowDecision;

/// LP results over the test slice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MlMetrics {
    /// Fraction of test decisions where the L3 thresholded score agrees with
    /// the L2 GTMX+ decision actually taken.
    pub decision_agreement: f64,
    /// Brier score of the raw probability against actual delivery (lower =
    /// better calibration), over the test slice.
    pub brier_score: f64,
    /// Decision threshold calibrated on the train slice.
    pub threshold_used: f32,
    /// Number of shadow decisions in each slice.
    pub n_train: usize,
    pub n_test: usize,
}

impl Default for MlMetrics {
    fn default() -> Self {
        MlMetrics {
            decision_agreement: 0.0,
            brier_score: 1.0,
            threshold_used: 0.5,
            n_train: 0,
            n_test: 0,
        }
    }
}

/// Run the LP shadow experiment over recorded decisions.
///
/// `delivered_ids` = message ids that were delivered within TTL (from the
/// same simulation's delivery log — the supervision signal). `split_at_ms`
/// separates the train (early) from test (late) timeline.
pub fn run_shadow_experiment(
    samples: &[ShadowDecision],
    delivered_ids: &HashSet<MessageId>,
    split_at_ms: u64,
) -> MlMetrics {
    let train: Vec<&ShadowDecision> = samples.iter().filter(|s| s.at_ms < split_at_ms).collect();
    let test: Vec<&ShadowDecision> = samples.iter().filter(|s| s.at_ms >= split_at_ms).collect();
    if train.is_empty() || test.is_empty() {
        return MlMetrics {
            n_train: train.len(),
            n_test: test.len(),
            ..MlMetrics::default()
        };
    }

    let labelled = |s: &ShadowDecision| delivered_ids.contains(&s.msg_id);
    let fit_data: Vec<(FeatureVec, bool)> =
        train.iter().map(|s| (s.features, labelled(s))).collect();
    let predictor = LinearPredictor::fit(&fit_data);

    // Calibrate the decision threshold on the TRAIN slice only (no leakage).
    let mut best = 0.5f32;
    let mut best_agree = -1.0f64;
    let mut t = 0.05f32;
    while t < 1.0 {
        let agree = agreement(&train, &predictor, t, labelled);
        if agree > best_agree {
            best_agree = agree;
            best = t;
        }
        t += 0.05;
    }

    let decision_agreement = agreement(&test, &predictor, best, labelled);
    let brier_score = test
        .iter()
        .map(|s| {
            let p = predictor.score(&s.features);
            let y = labelled(s) as u32 as f32;
            (p - y) * (p - y)
        })
        .sum::<f32>()
        / test.len() as f32;

    MlMetrics {
        decision_agreement,
        brier_score: brier_score as f64,
        threshold_used: best,
        n_train: train.len(),
        n_test: test.len(),
    }
}

fn agreement(
    samples: &[&ShadowDecision],
    predictor: &LinearPredictor,
    threshold: f32,
    _labelled: impl Fn(&ShadowDecision) -> bool,
) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let agree = samples
        .iter()
        .filter(|s| (predictor.score(&s.features) >= threshold) == s.l2_decided)
        .count();
    agree as f64 / samples.len() as f64
}

/// GT (gateway-egress timing) evaluation.
///
/// `gateway_contact_times_ms` = the gateway node's contact times (sorted).
/// `split_at_ms` = train/test boundary. GT fits the robust **median** period
/// on the train slice and predicts the next contact; the static baseline uses
/// the **mean** interval (GW-001 static weights carry no timing/robustness).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GtEval {
    /// Mean absolute timing error of the learned GT predictor (sim-ms).
    pub gt_mean_err_ms: f64,
    /// Mean absolute timing error of the non-robust static baseline (sim-ms).
    pub static_mean_err_ms: f64,
    /// Median period learned by GT.
    pub learned_period_ms: f64,
}

impl Default for GtEval {
    fn default() -> Self {
        GtEval {
            gt_mean_err_ms: 0.0,
            static_mean_err_ms: 0.0,
            learned_period_ms: 0.0,
        }
    }
}

pub fn evaluate_gt(gateway_contact_times_ms: &[u64], split_at_ms: u64) -> GtEval {
    // Contact times must be monotonic for interval math (ML-RT-03): a sorted
    // copy prevents u64 underflow on `w[1]-w[0]` from out-of-order input.
    let mut sorted: Vec<u64> = gateway_contact_times_ms.to_vec();
    sorted.sort_unstable();
    let train: Vec<u64> = sorted
        .iter()
        .copied()
        .filter(|t| *t < split_at_ms)
        .collect();
    let test: Vec<u64> = sorted
        .iter()
        .copied()
        .filter(|t| *t >= split_at_ms)
        .collect();
    if train.len() < 2 || test.is_empty() {
        return GtEval::default();
    }

    let predictor = GtPredictor::fit(&train);
    // Static baseline: mean inter-contact interval (no robustness).
    let intervals: Vec<f64> = train.windows(2).map(|w| (w[1] - w[0]) as f64).collect();
    let static_period = intervals.iter().sum::<f64>() / intervals.len() as f64;

    let mut gt_err = 0.0f64;
    let mut static_err = 0.0f64;
    let mut prev = *train.last().unwrap();
    for t in &test {
        let actual = *t as f64;
        gt_err += (predictor.predict_next(prev) as f64 - actual).abs();
        static_err += (prev as f64 + static_period - actual).abs();
        prev = *t;
    }
    let n = test.len() as f64;
    GtEval {
        gt_mean_err_ms: gt_err / n,
        static_mean_err_ms: static_err / n,
        learned_period_ms: predictor.period_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_experiment_returns_default_metrics() {
        let m = run_shadow_experiment(&[], &HashSet::new(), 1000);
        assert_eq!(m.n_train, 0);
        assert_eq!(m.n_test, 0);
    }

    #[test]
    fn gt_beats_static_baseline_on_periodic_with_outlier() {
        // Periodic schedule with ONE long outlier gap inside the TRAIN slice:
        // the mean is skewed, the robust median is not → GT wins. The outlier
        // must not collide with an existing timestamp or `dedup` collapses it.
        let train: Vec<u64> =
            [0, 1000, 2000, 3000, 4000, 9000, 10000, 11000, 12000, 13000].to_vec(); // gap of 5000 between 4000 and 9000, then periodic
        let test: Vec<u64> = (0..5).map(|i| 14_000 + i * 1000).collect(); // 14000..=18000
        let mut times = train;
        times.extend(test.iter().copied());
        let eval = evaluate_gt(&times, 14_000);
        assert!(
            eval.gt_mean_err_ms < eval.static_mean_err_ms,
            "median must beat mean under an outlier: {eval:?}"
        );
    }

    #[test]
    fn gt_parity_on_aperiodic_schedule() {
        // Random-ish intervals → GT must NOT strictly beat the static mean
        // baseline (the "no-help bound", ML-001 AC-4).
        let times: Vec<u64> = [0, 830, 1750, 2600, 3490, 4700, 5600, 6650, 7590, 8800].to_vec();
        let eval = evaluate_gt(&times, 5000);
        // Learned period ≈ static mean within a small tolerance (parity).
        let diff = (eval.gt_mean_err_ms - eval.static_mean_err_ms).abs();
        let scale = eval.static_mean_err_ms.max(1.0);
        assert!(
            diff / scale < 0.35,
            "GT should be at parity on aperiodic contacts, got {eval:?}"
        );
    }

    #[test]
    fn gt_tolerates_unsorted_contact_times() {
        // ML-RT-03 regression: out-of-order input must not underflow the
        // interval subtraction (u64) or panic.
        let mut times: Vec<u64> = [0, 1000, 2000, 3000, 4000, 14_000, 15_000, 16_000].to_vec();
        times.reverse(); // deliberately unsorted
        let eval = evaluate_gt(&times, 10_000);
        assert!(
            eval.gt_mean_err_ms >= 0.0 && eval.static_mean_err_ms >= 0.0,
            "errors must be non-negative after defensive sort: {eval:?}"
        );
        assert!(eval.learned_period_ms > 0.0, "period must be learned");
    }
}
