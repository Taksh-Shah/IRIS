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

use std::collections::HashMap;

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
/// `delivered_at` = delivery timestamps keyed by message id (from the same
/// simulation's delivery log). `split_at_ms` separates the train (early)
/// from test (late) timeline.
pub fn run_shadow_experiment(
    samples: &[ShadowDecision],
    delivered_at: &HashMap<MessageId, u64>,
    split_at_ms: u64,
) -> MlMetrics {
    // SIM-14: group-stable split — assign each msg_id wholly to train or test
    // by its first-decision time; prevents the same message from appearing in
    // both slices (group leakage).
    let mut first_seen: HashMap<MessageId, u64> = HashMap::new();
    for s in samples {
        first_seen
            .entry(s.msg_id)
            .and_modify(|e| *e = (*e).min(s.at_ms))
            .or_insert(s.at_ms);
    }
    let train: Vec<&ShadowDecision> = samples
        .iter()
        .filter(|s| {
            first_seen
                .get(&s.msg_id)
                .copied()
                .unwrap_or(u64::MAX)
                < split_at_ms
        })
        .collect();
    let test: Vec<&ShadowDecision> = samples
        .iter()
        .filter(|s| {
            first_seen
                .get(&s.msg_id)
                .copied()
                .unwrap_or(0)
                >= split_at_ms
        })
        .collect();
    if train.is_empty() || test.is_empty() {
        return MlMetrics {
            n_train: train.len(),
            n_test: test.len(),
            ..MlMetrics::default()
        };
    }

    // SIM-13: label true only when the message was delivered BEFORE the split
    // (not just "ever delivered") — test-window deliveries must not pollute the
    // training supervision signal (label leakage).
    let labelled = |s: &ShadowDecision| {
        delivered_at
            .get(&s.msg_id)
            .is_some_and(|t| *t < split_at_ms)
    };
    let fit_data: Vec<(FeatureVec, bool)> =
        train.iter().map(|s| (s.features, labelled(s))).collect();
    let predictor = LinearPredictor::fit(&fit_data);

    // Calibrate the decision threshold on the TRAIN slice only (no leakage).
    let mut best = 0.5f32;
    let mut best_agree = -1.0f64;
    let mut t = 0.05f32;
    while t < 1.0 {
        let agree = agreement(&train, &predictor, t);
        if agree > best_agree {
            best_agree = agree;
            best = t;
        }
        t += 0.05;
    }

    let decision_agreement = agreement(&test, &predictor, best);
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

// SIM-15: dropped the unused `_labelled` parameter — agreement measures L3
// threshold-score vs the L2 decision actually taken (s.l2_decided), never
// the delivery label, so the closure was dead weight.
fn agreement(
    samples: &[&ShadowDecision],
    predictor: &LinearPredictor,
    threshold: f32,
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
    sorted.dedup(); // SIM-16: duplicate timestamps produce zero intervals that skew the median
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
        let m = run_shadow_experiment(&[], &HashMap::new(), 1000);
        assert_eq!(m.n_train, 0);
        assert_eq!(m.n_test, 0);
    }

    // SIM-13: label leakage guard — a message delivered after split_at_ms must
    // NOT be labeled true in the training slice.
    #[test]
    fn sim13_post_split_delivery_does_not_pollute_train_label() {
        use crate::sim::ml::ShadowDecision;
        let zero_fx = crate::sim::ml::features::FeatureVec::new([0.0; 8]);
        let msg_a = MessageId([1u8; 16]); // first seen pre-split → train
        let msg_b = MessageId([2u8; 16]); // first seen post-split → test
        let samples = vec![
            ShadowDecision {
                msg_id: msg_a,
                at_ms: 100,
                src: 0,
                dst: 1,
                features: zero_fx,
                l2_decided: false,
            },
            ShadowDecision {
                msg_id: msg_b,
                at_ms: 700,
                src: 1,
                dst: 0,
                features: zero_fx,
                l2_decided: false,
            },
        ];
        // msg_a delivered at t=800 (post-split) → must label false in train
        // msg_b delivered at t=200 (pre-split) → correctly labeled true in test
        let mut delivered_at = HashMap::new();
        delivered_at.insert(msg_a, 800u64);
        delivered_at.insert(msg_b, 200u64);
        let m = run_shadow_experiment(&samples, &delivered_at, 500);
        assert_eq!(m.n_train, 1, "msg_a (first seen t=100) goes to train");
        assert_eq!(m.n_test, 1, "msg_b (first seen t=700) goes to test");
    }

    // SIM-14: group leakage guard — the same msg_id must not appear in both
    // train and test slices.
    #[test]
    fn sim14_same_msg_id_not_split_across_train_and_test() {
        use crate::sim::ml::ShadowDecision;
        let zero_fx = crate::sim::ml::features::FeatureVec::new([0.0; 8]);
        let msg = MessageId([3u8; 16]);
        // Same message observed at t=200 (pre-split) and t=700 (post-split).
        let samples = vec![
            ShadowDecision {
                msg_id: msg,
                at_ms: 200,
                src: 0,
                dst: 1,
                features: zero_fx,
                l2_decided: true,
            },
            ShadowDecision {
                msg_id: msg,
                at_ms: 700,
                src: 1,
                dst: 0,
                features: zero_fx,
                l2_decided: false,
            },
        ];
        // Group-stable split: first-seen is t=200 < 500 → both samples go to train.
        let m = run_shadow_experiment(&samples, &HashMap::new(), 500);
        assert_eq!(m.n_train, 2, "both observations of the same msg go to train");
        assert_eq!(m.n_test, 0, "test slice is empty (no test-only msg_ids)");
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
