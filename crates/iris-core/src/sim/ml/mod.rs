//! ML-001 — L3 experimental shadow predictors (ML-001, WP-11).
//!
//! Purely experimental, in-simulation learning layer that evaluates whether
//! learned delivery-probability / gateway-egress prediction can match or beat
//! the RFC 6693 PRoPHET v2 L2 baseline — and when it cannot (RES-0014,
//! ML_DESIGN.md).
//!
//! Governing constraints (ARCHITECTURAL_PRINCIPLES #5/#6, ML_DESIGN.md §1):
//! - **Shadow-only**: no code path from L3 output to actual P0/P1 forwarding.
//!   The L2 decision actually taken is unchanged; L3 records what it *would
//!   have* decided (`ShadowDecision`).
//! - **Offline training in-sim**: no external ML runtime; pure-std arithmetic.
//! - **In-band features only**: no new sensors, no wire/protocol changes.
//! - **Deterministic**: same seed → same features/decisions (`FeatureVec` is a
//!   pure function of node state).

use crate::protocol::MessageId;

pub mod experiment;
pub mod features;
pub mod predictor;

pub use experiment::{evaluate_gt, run_shadow_experiment, GtEval, MlMetrics};
pub use features::FeatureVec;
pub use predictor::{GtPredictor, LinearPredictor};

/// One L3 shadow decision recorded at an L2 decision point (no forwarding
/// impact). `l2_decided` is what GTMX+/spray actually chose; `features` is
/// what the L3 predictor scored at the same instant.
#[derive(Debug, Clone)]
pub struct ShadowDecision {
    pub msg_id: MessageId,
    /// Sim-ms of the decision (used for the temporal train/test split).
    pub at_ms: u64,
    pub src: usize,
    pub dst: usize,
    pub features: FeatureVec,
    pub l2_decided: bool,
}

/// Shadow observer collecting [`ShadowDecision`]s during a [`Simulation`]
/// run. Defaults to empty; attaching it must not change forwarding outcomes
/// (verified by ML-001 AC-2).
#[derive(Debug, Default)]
pub struct ShadowRecorder {
    pub samples: Vec<ShadowDecision>,
}

impl ShadowRecorder {
    pub fn new() -> Self {
        ShadowRecorder {
            samples: Vec::new(),
        }
    }

    pub fn record(&mut self, d: ShadowDecision) {
        self.samples.push(d);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
