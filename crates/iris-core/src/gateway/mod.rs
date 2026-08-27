//! Gateway discovery & selection engine — GW-001 (WP-7).
//!
//! Per `docs/routing/GATEWAY_SELECTION.md` + `docs/architecture/GATEWAY_ARCHITECTURE.md`:
//! a gateway is any IRIS node with an uplink transport (Internet/relay, LoRa
//! backbone, satellite) not shared by all peers. GW-001 provides:
//!
//! 1. **Detection** — a node self-advertises gateway capability when its
//!    INTERNET-001 transport is Connected; peers advertise the `gateway`
//!    capability tag in the DISCO-001 capability handshake.
//! 2. **Registry** — [`GatewayManager`] tracks known gateways with per-type
//!    quality metrics and hop distance, reconciled read-only against the
//!    DISCO-001 [`NeighborTable`].
//! 3. **Selection** — deterministic scoring + a priority-strategy matrix:
//!    P0 → all gateways (multi-path), P1–P2 → primary+backup, P3+ → single
//!    best, none → mesh-only fallback (`GatewaySelection::None`).
//! 4. **Failure handling** — [`GatewayHealthMonitor`]: consecutive ACK
//!    timeouts mark a gateway failed (excluded); advertisements re-admit it
//!    in probation (0.5 score factor) until it proves out via ACK success.
//!
//! Seams consumed (no edits to their modules): DISCO-001 `CapabilityBundle`
//! tags + `NeighborTable`, INTERNET-001 `TransportCost`+state via
//! [`self_internet_gateway`], and the existing `TopologyEvent::GatewayChanged`
//! channel.
//!
//! **Integration status (MG-30):** this module is feature-complete and
//! unit-tested, but [`GatewayManager`] has no production caller anywhere in
//! the tree today — nothing constructs one outside this file's own tests,
//! and its upstream feed (`discovery::mod::ingest_handshake`, which is what
//! would populate a neighbor's `capabilities` bundle) has no production
//! caller either. Wiring this into the composition root (constructing the
//! manager, calling `reconcile` on a discovery tick, routing
//! `deliver_outbound`'s off-mesh decision through `select`, and calling
//! `ingest_handshake` after signature verification) is a deliberate,
//! separate architectural decision this pass does not make: it would
//! activate a new, security-sensitive live-routing path (every off-mesh
//! message potentially funneled through gateway selection) touching the
//! message engine's send/receive decision points, not a contained change to
//! this file. MG-25/26/27/28/29 (the trust/health/scoring correctness
//! defects that would matter *the moment* this is wired) are fixed and
//! ready for that integration to build on.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::broadcast;

use crate::discovery::neighbor_table::NeighborTable;
use crate::message::MessagePriority;
use crate::message::PeerId;
use crate::observability::event;
use crate::transport::TopologyEvent;

/// Discount applied to a gateway on the failed→probation transition.
pub const PROBATION_SCORE_FACTOR: f32 = 0.5;
/// Failure threshold: consecutive ACK timeouts that mark a gateway failed.
pub const FAILURE_THRESHOLD: u32 = 3;
/// Kept for observability (when a gateway entered probation) — MG-28: no
/// longer gates a return to full score by itself; only `record_success` does.
pub const RECOVERY_WINDOW: Duration = Duration::from_secs(300);
/// Upper bound on tracked gateway candidates (eviction of oldest on overflow).
pub const MAX_GATEWAY_CANDIDATES: usize = 64;
/// Gateways scoring below this floor are excluded from selection (NaN/empty).
pub const MIN_QUALITY_FLOOR: f32 = 0.01;
/// MG-25/27/29: score multiplier for a gateway with zero observed successful
/// deliveries. An unmeasured, self-asserted gateway (a bare capability tag,
/// unauthenticated — MG-25) must not score as if fully proven: this is what
/// let a freshly-adopted attacker gateway win selection outright.
pub const MIN_CONFIDENCE: f32 = 0.3;
/// Successful deliveries after which [`GatewayHealthMonitor::confidence_factor`]
/// saturates at 1.0.
pub const CONFIDENCE_SATURATION_SAMPLES: u32 = 5;
/// MG-27: no real network hop is 0 ms; a floor here stops an advertised
/// `latency_ms: 0` from claiming the maximum latency factor.
pub const MIN_PLAUSIBLE_LATENCY_MS: u32 = 1;
/// MG-34: multiplicative score penalty per hop distance (a direct neighbor
/// is hop 0, `HOP_DISCOUNT^0 = 1.0`; each additional hop compounds). Applied
/// in `select_inner`, not inside [`compute_gateway_quality`] — the score
/// function stays a pure function of one capability, testable and callable
/// without a full `GatewayCandidate`.
pub const HOP_DISCOUNT: f32 = 0.85;
/// MG-26: bound on health-monitor entries kept *only* because they are
/// quarantined (`Failed`/`HardFailed` but no longer a known gateway) — an
/// attacker cycling through fresh PeerIds must not be able to grow this
/// bookkeeping without limit now that quarantine survives withdrawal.
pub const MAX_QUARANTINE_ENTRIES: usize = 256;

/// Capability tag used in DISCO-001 handshake bundles for any gateway.
pub const GATEWAY_TAG: &str = "gateway";
/// Type-specific tag suffix: `gateway:internet`, `gateway:lora`, `gateway:satellite`.
pub const GATEWAY_TAG_INTERNET: &str = "gateway:internet";
pub const GATEWAY_TAG_LORA: &str = "gateway:lora";
pub const GATEWAY_TAG_SATELLITE: &str = "gateway:satellite";

/// Classification of the uplink a gateway bridges to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GatewayType {
    Internet,
    Lora,
    Satellite,
    UsbEthernet,
}

impl GatewayType {
    /// True when the uplink is subject to regulatory duty-cycle limits
    /// (LoRa ISM bands; see GATEWAY_ARCHITECTURE.md).
    pub fn has_duty_cycle(self) -> bool {
        matches!(self, GatewayType::Lora)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            GatewayType::Internet => "internet",
            GatewayType::Lora => "lora",
            GatewayType::Satellite => "satellite",
            GatewayType::UsbEthernet => "usb-ethernet",
        }
    }

    /// The worst priority this gateway type will serve (P0 numeric 0 is the
    /// best; everything numerically ≤ `max_priority` is served).
    pub fn default_max_priority(self) -> MessagePriority {
        match self {
            GatewayType::Internet | GatewayType::UsbEthernet => MessagePriority::P7,
            GatewayType::Lora => MessagePriority::P3,
            GatewayType::Satellite => MessagePriority::P2,
        }
    }
}

/// A gateway's advertised uplink capability (GATEWAY_SELECTION.md §Gateway
/// Capability). Fields are modeled inputs; service refines them with transport
/// ACK/probe telemetry.
#[derive(Debug, Clone, PartialEq)]
pub struct GatewayCapability {
    pub gateway_type: GatewayType,
    pub transport_id: String,
    /// Advertised uplink bandwidth, bps.
    pub bandwidth_bps: u64,
    /// Advertised one-way latency, ms.
    pub latency_ms: u32,
    /// Reliability estimate 0.0–1.0 (historical ACK rate).
    pub reliability_score: f32,
    /// Cost factor 0.0 = free, 1.0 = very expensive.
    pub cost_factor: f32,
    /// Current load 0.0–1.0.
    pub current_load: f32,
    /// Messages currently queued for the uplink.
    pub queue_depth: u32,
    /// Fraction of capacity remaining (LoRa duty cycle); 1.0 otherwise.
    pub duty_cycle_remaining: f32,
    /// Worst priority served; all numerically-equal-or-better are served.
    pub max_priority: MessagePriority,
}

impl GatewayCapability {
    pub fn new(gateway_type: GatewayType, transport_id: impl Into<String>) -> Self {
        GatewayCapability {
            gateway_type,
            transport_id: transport_id.into(),
            // `bandwidth_bps: 0` and `queue_depth: 0` are already pessimistic
            // (0 bandwidth scores worst; 0 queue depth only matters once
            // `current_load` below is no longer maxed-out-optimistic).
            bandwidth_bps: 0,
            // MG-29: unmeasured must not score as *better* than measured.
            // `0` here used to make latency/cost/load — 65% of the weight
            // budget — read as *perfect* for a gateway nobody has ever
            // actually observed. u32::MAX drives latency_factor to 0 until
            // this gateway is actually probed.
            latency_ms: u32::MAX,
            reliability_score: 1.0,
            // 0.5: unknown cost is not the same claim as "free".
            cost_factor: 0.5,
            // 0.5: unknown load is not the same claim as "idle".
            current_load: 0.5,
            queue_depth: 0,
            duty_cycle_remaining: 1.0,
            max_priority: gateway_type.default_max_priority(),
        }
    }

    /// True when this gateway will serve `priority` (GATEWAY_SELECTION.md
    /// §Priority Compatibility — enforced, not preferred).
    pub fn serves_priority(&self, priority: MessagePriority) -> bool {
        priority.as_u8() <= self.max_priority.as_u8()
    }
}

/// A known gateway candidate (GATEWAY_SELECTION.md §Gateway Discovery).
#[derive(Debug, Clone)]
pub struct GatewayCandidate {
    pub node_id: PeerId,
    pub capability: GatewayCapability,
    /// Hop distance: 0 = direct neighbor advertisement. Scored in
    /// `select_inner` via a per-hop discount (MG-34) — always 0 today since
    /// multi-hop propagation is deferred (GATEWAY_DESIGN.md:33-34), but
    /// correct by construction once it lands.
    pub hop_count: u8,
    pub discovered_at: Instant,
    pub last_confirmed: Instant,
}

/// Selection output (GATEWAY_SELECTION.md §Selection Algorithm).
#[derive(Debug, Clone, PartialEq)]
pub enum GatewaySelection {
    /// No compatible gateway available — mesh-only fallback; the message must
    /// not stall (SCF carries / flood continues).
    None { reason: String },
    /// P0: use every eligible gateway simultaneously (multi-path; first
    /// delivery wins, recipient dedup).
    All { gateways: Vec<PeerId> },
    /// P1–P2: primary plus one backup.
    WithBackup {
        primary: PeerId,
        backup: PeerId,
        primary_score: f32,
    },
    /// P3+: single best gateway.
    Single { gateway: PeerId, score: f32 },
}

impl GatewaySelection {
    /// The gateway(s) explicitly chosen, if any.
    pub fn chosen_gateways(&self) -> Vec<PeerId> {
        match self {
            GatewaySelection::None { .. } => Vec::new(),
            GatewaySelection::All { gateways } => gateways.clone(),
            GatewaySelection::WithBackup {
                primary, backup, ..
            } => vec![*primary, *backup],
            GatewaySelection::Single { gateway, .. } => vec![*gateway],
        }
    }
}

/// Upper bound used to normalize bandwidth into 0.0–1.0
/// (INTERNET-001 conservative 100 Mbps ceiling).
pub const MAX_EXPECTED_BPS: u64 = 100_000_000;
/// Latency at/above which the latency factor collapses to 0 (60 s).
pub const MAX_EXPECTED_LATENCY_MS: u32 = 60_000;

/// Deterministic quality score for one gateway against a message priority
/// (GATEWAY_SELECTION.md §Gateway Quality Score).
pub fn compute_gateway_quality(gw: &GatewayCapability, msg_priority: MessagePriority) -> f32 {
    if !gw.serves_priority(msg_priority) {
        return 0.0;
    }
    let reliability = gw.reliability_score.clamp(0.0, 1.0);
    let bandwidth_factor = ((gw.bandwidth_bps as f32 / MAX_EXPECTED_BPS as f32).min(1.0)).sqrt();
    let latency_factor = 1.0 - (gw.latency_ms as f32 / MAX_EXPECTED_LATENCY_MS as f32).min(1.0);
    let cost_factor = 1.0 - gw.cost_factor.clamp(0.0, 1.0);
    let availability =
        (1.0 - gw.current_load.clamp(0.0, 1.0)) * (1.0 - gw.queue_depth as f32 / 100.0).max(0.0);
    let duty_cycle_ok = if gw.gateway_type.has_duty_cycle() {
        gw.duty_cycle_remaining.clamp(0.0, 1.0)
    } else {
        1.0
    };
    0.30 * reliability
        + 0.20 * bandwidth_factor
        + 0.15 * latency_factor
        + 0.20 * cost_factor
        + 0.10 * availability
        + 0.05 * duty_cycle_ok
}

/// Health state of one gateway (red-team hardened).
///
/// A rogue that re-advertises after failing must not loop back to selection
/// forever: re-admission into probation does **not** reset the failure counter,
/// and repeated failed→probation cycles escalate to `HardFailed`, which only an
/// explicit ACK success (probe pass) can clear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GatewayHealthState {
    /// Fully trusted — full score.
    Healthy,
    /// Re-admitted after failure, score discounted, must prove out via ACK.
    Probation,
    /// Disabled from selection (failed the ACK-threshold again in probation).
    Failed,
    /// Hard-excluded: repeated re-admission abuse; needs explicit success.
    HardFailed,
}

/// Discrete failure/health state per gateway.
///
/// Failure is *outcome-based* (consecutive ACK timeouts), not just
/// connectivity-based, so a connected gateway that drops messages is removed
/// from consideration (GATEWAY_ARCHITECTURE.md §Gateway Quality Tracking, §
/// Failure Handling).
///
/// Deliberately **not** `Clone` (MG-38): `failure_counters`/
/// `re_admission_strikes` are accumulating adversary evidence — a deep copy
/// would let strikes recorded on one instance go invisible to another,
/// silently defeating `HARD_FAIL_STRIKES` escalation for whichever clone a
/// caller happens to hold. Share via `Arc<RwLock<GatewayManager>>` (matching
/// `TransportManager`, which is deliberately not `Clone` either) instead of
/// cloning.
#[derive(Debug)]
pub struct GatewayHealthMonitor {
    states: HashMap<PeerId, GatewayHealthState>,
    failure_counters: HashMap<PeerId, u32>,
    re_admission_strikes: HashMap<PeerId, u32>,
    /// Kept for observability (when a gateway entered probation) — no longer
    /// consulted by [`GatewayHealthMonitor::score_factor`] (MG-28: elapsed
    /// time must never by itself restore full trust, only `record_success`
    /// does).
    probation_until: HashMap<PeerId, Instant>,
    /// MG-25/27/29: count of successful deliveries ever recorded for this
    /// gateway, feeding [`GatewayHealthMonitor::confidence_factor`]. Not
    /// reset by failure — a proven track record should not evaporate on one
    /// bad delivery (health state already excludes a failed gateway
    /// independently of this).
    success_counts: HashMap<PeerId, u32>,
}

impl Default for GatewayHealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayHealthMonitor {
    pub fn new() -> Self {
        GatewayHealthMonitor {
            states: HashMap::new(),
            failure_counters: HashMap::new(),
            re_admission_strikes: HashMap::new(),
            probation_until: HashMap::new(),
            success_counts: HashMap::new(),
        }
    }

    /// Record a delivery ACK timeout for `gateway`. Returns `true` when this
    /// timeout flips the gateway into `Failed`.
    pub fn record_ack_timeout(&mut self, gateway: PeerId) -> bool {
        let state = self
            .states
            .get(&gateway)
            .copied()
            .unwrap_or(GatewayHealthState::Healthy);
        match state {
            GatewayHealthState::HardFailed => false,
            GatewayHealthState::Healthy | GatewayHealthState::Probation => {
                // NOTE: never reset on advertisement — a blackhole that
                // re-advertises cannot erase accumulated strikes (REDTEAM-01).
                let count = self.failure_counters.entry(gateway).or_insert(0);
                *count += 1;
                if *count >= FAILURE_THRESHOLD {
                    match state {
                        GatewayHealthState::Probation => {
                            // Failed again while proving out: escalate.
                            let strikes = self.re_admission_strikes.entry(gateway).or_insert(0);
                            *strikes += 1;
                            if *strikes >= HARD_FAIL_STRIKES {
                                self.states.insert(gateway, GatewayHealthState::HardFailed);
                                self.probation_until.remove(&gateway);
                                tracing::warn!(
                                    event = event::GW_HEALTH_TRANSITION,
                                    gateway = %gateway.short(),
                                    from = "Probation",
                                    to = "HardFailed",
                                    "gateway hard-failed"
                                );
                            } else {
                                self.states.insert(gateway, GatewayHealthState::Failed);
                                tracing::warn!(
                                    event = event::GW_HEALTH_TRANSITION,
                                    gateway = %gateway.short(),
                                    from = "Probation",
                                    to = "Failed",
                                    "gateway failed on probation"
                                );
                            }
                        }
                        _ => {
                            self.states.insert(gateway, GatewayHealthState::Failed);
                            tracing::warn!(
                                event = event::GW_HEALTH_TRANSITION,
                                gateway = %gateway.short(),
                                from = "Healthy",
                                to = "Failed",
                                "gateway marked failed"
                            );
                        }
                    }
                    true
                } else {
                    false
                }
            }
            GatewayHealthState::Failed => false, // already excluded
        }
    }

    /// Record a successful delivery via `gateway`: returns the gateway to full
    /// trust, resetting failure counter and re-admission strikes.
    pub fn record_success(&mut self, gateway: PeerId) {
        self.failure_counters.insert(gateway, 0);
        self.re_admission_strikes.insert(gateway, 0);
        self.states.insert(gateway, GatewayHealthState::Healthy);
        self.probation_until.remove(&gateway);
        *self.success_counts.entry(gateway).or_insert(0) += 1;
        tracing::debug!(
            event = event::GW_HEALTH_TRANSITION,
            gateway = %gateway.short(),
            to = "Healthy",
            "gateway health restored"
        );
    }

    /// Count of entries kept only because they are quarantined
    /// (`Failed`/`HardFailed`, no longer a known gateway) — for metrics and
    /// tests verifying [`MAX_QUARANTINE_ENTRIES`] holds. O(n) in table size.
    pub fn quarantine_len(&self) -> usize {
        self.states
            .values()
            .filter(|s| matches!(s, GatewayHealthState::Failed | GatewayHealthState::HardFailed))
            .count()
    }

    /// MG-25/27/29: score multiplier that starts at [`MIN_CONFIDENCE`] for a
    /// gateway with no recorded successful delivery and rises to `1.0` after
    /// [`CONFIDENCE_SATURATION_SAMPLES`] of them. Applied alongside (not
    /// instead of) [`GatewayHealthMonitor::score_factor`] — this is about
    /// *how proven* a gateway is, which is orthogonal to whether it is
    /// currently healthy.
    pub fn confidence_factor(&self, gateway: &PeerId) -> f32 {
        let successes = self.success_counts.get(gateway).copied().unwrap_or(0);
        let ramp = successes.min(CONFIDENCE_SATURATION_SAMPLES) as f32
            / CONFIDENCE_SATURATION_SAMPLES as f32;
        MIN_CONFIDENCE + (1.0 - MIN_CONFIDENCE) * ramp
    }

    /// A gateway sent a fresh advertisement. Governs the failed→probation
    /// transition: re-admission does not reset failure counters, the probation
    /// window is set once (not refreshed each pass), and a HardFailed gateway
    /// stays excluded until an explicit `record_success` (probe pass).
    pub fn handle_advertisement(&mut self, gateway: PeerId) {
        match self.states.get(&gateway).copied() {
            Some(GatewayHealthState::Failed) => {
                self.states.insert(gateway, GatewayHealthState::Probation);
                // Set the window ONCE; reconcile re-passes must not extend it.
                self.probation_until
                    .entry(gateway)
                    .or_insert_with(|| Instant::now() + RECOVERY_WINDOW);
                tracing::debug!(
                    event = event::GW_HEALTH_TRANSITION,
                    gateway = %gateway.short(),
                    from = "Failed",
                    to = "Probation",
                    "gateway re-admitted in probation"
                );
            }
            Some(GatewayHealthState::HardFailed) => { /* excluded until proven */ }
            _ => {}
        }
    }

    /// Whether this gateway is currently excluded from selection
    /// (`Failed` or `HardFailed`).
    pub fn is_failed(&self, gateway: &PeerId) -> bool {
        matches!(
            self.states.get(gateway),
            Some(GatewayHealthState::Failed) | Some(GatewayHealthState::HardFailed)
        )
    }

    /// Whether this gateway is in probation (admitted again but discounted).
    pub fn is_probation(&self, gateway: &PeerId) -> bool {
        self.states.get(gateway) == Some(&GatewayHealthState::Probation)
    }

    /// Score multiplier: 1.0 healthy, 0.5 in probation, 0.0 failed/hard-failed.
    ///
    /// MG-28: probation is discounted *until a successful delivery proves the
    /// gateway out* — [`GatewayHealthMonitor::record_success`] is the only
    /// path back to 1.0. Elapsed time alone must never restore full trust
    /// (the previous behaviour let a re-advertising blackhole sit at a 0.5
    /// discount for `RECOVERY_WINDOW` and then earn full trust for free,
    /// having delivered nothing). `now` kept in the signature for API
    /// stability even though this arm no longer consults it — a caller
    /// injects it the same way as before.
    pub fn score_factor(&self, gateway: &PeerId, _now: Instant) -> f32 {
        match self.states.get(gateway) {
            Some(GatewayHealthState::Failed) | Some(GatewayHealthState::HardFailed) => 0.0,
            Some(GatewayHealthState::Probation) => PROBATION_SCORE_FACTOR,
            _ => 1.0,
        }
    }

    /// Drop bookkeeping for gateways no longer known.
    ///
    /// MG-26: a `Failed`/`HardFailed` record is a *security* record, not
    /// memory hygiene — its entire purpose is to outlive the subject's
    /// presence, so a blackhole cannot clear its strikes by simply dropping
    /// off the neighbor table for one reconcile cycle and re-advertising.
    /// Only genuinely-gone entries that were never excluded are dropped;
    /// quarantined entries are kept (bounded by [`MAX_QUARANTINE_ENTRIES`],
    /// so an attacker cycling through fresh PeerIds cannot grow this
    /// bookkeeping without limit).
    pub fn prune(&mut self, known: &[PeerId]) {
        self.states.retain(|id, s| {
            known.contains(id)
                || matches!(s, GatewayHealthState::Failed | GatewayHealthState::HardFailed)
        });
        self.failure_counters
            .retain(|id, _| self.states.contains_key(id));
        self.re_admission_strikes
            .retain(|id, _| self.states.contains_key(id));
        self.probation_until.retain(|id, _| known.contains(id));
        self.success_counts.retain(|id, _| known.contains(id));

        // Bound the quarantine: entries kept only because they are
        // Failed/HardFailed (not because they're a known gateway).
        let quarantined: Vec<PeerId> = self
            .states
            .keys()
            .filter(|id| !known.contains(id))
            .copied()
            .collect();
        if quarantined.len() > MAX_QUARANTINE_ENTRIES {
            let mut excess = quarantined.len() - MAX_QUARANTINE_ENTRIES;
            // Trim plain `Failed` first (the weaker signal); `HardFailed`
            // is the stronger record and is only trimmed as a last resort.
            for tier in [GatewayHealthState::Failed, GatewayHealthState::HardFailed] {
                if excess == 0 {
                    break;
                }
                let victims: Vec<PeerId> = quarantined
                    .iter()
                    .filter(|id| self.states.get(id) == Some(&tier))
                    .take(excess)
                    .copied()
                    .collect();
                for id in &victims {
                    self.states.remove(id);
                    self.failure_counters.remove(id);
                    self.re_admission_strikes.remove(id);
                }
                excess -= victims.len();
            }
        }
    }
}

/// Re-admission cycles that escalate a probation-recidivist to hard-exclusion.
pub const HARD_FAIL_STRIKES: u32 = 2;

/// The gateway discovery + selection engine (GW-001).
///
/// Deliberately **not** `Clone` (MG-38, same reasoning as the embedded
/// [`GatewayHealthMonitor`]) — share via `Arc<RwLock<GatewayManager>>`.
#[derive(Debug)]
pub struct GatewayManager {
    gateways: HashMap<PeerId, GatewayCandidate>,
    health: GatewayHealthMonitor,
    /// Capability of *this node's own* uplink, if any (set from the
    /// INTERNET-001/LoRa transport state).
    self_capability: Option<GatewayCapability>,
    /// MG-32: this node's own identity, needed to make `self_capability` a
    /// selectable candidate rather than only an advertised fact. Optional
    /// (via [`GatewayManager::with_local_id`], not a required constructor
    /// arg) so the 13 existing `new()` call sites — all tests, since this
    /// engine has zero production callers today (MG-30) — are unaffected.
    local_id: Option<PeerId>,
    /// MG-31: gateway adoption/withdrawal/failure events, published here
    /// instead of only returned to a caller that (today) has nowhere to
    /// forward them — mirrors `TransportManager::topology_tx`.
    topology_tx: broadcast::Sender<TopologyEvent>,
    /// MG-33: the currently-elected single gateway per priority tier (P3+
    /// only — `WithBackup`/`All` have no single incumbent to be sticky
    /// about). A challenger must beat the incumbent by `SWITCH_MARGIN`
    /// before `select` actually switches, so a link-quality flap at a score
    /// boundary doesn't fragment an in-flight transfer across gateways.
    elected: HashMap<MessagePriority, PeerId>,
}

impl Default for GatewayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayManager {
    pub fn new() -> Self {
        let (topology_tx, _) = broadcast::channel(256);
        GatewayManager {
            gateways: HashMap::new(),
            health: GatewayHealthMonitor::new(),
            self_capability: None,
            local_id: None,
            topology_tx,
            elected: HashMap::new(),
        }
    }

    /// MG-32: attach this node's own identity, making `self_capability` (when
    /// set) a candidate in `select()` rather than only an advertised fact.
    pub fn with_local_id(mut self, id: PeerId) -> Self {
        self.local_id = Some(id);
        self
    }

    /// MG-31: subscribe to gateway adoption/withdrawal/failure events.
    pub fn topology_events(&self) -> broadcast::Receiver<TopologyEvent> {
        self.topology_tx.subscribe()
    }

    // --- Detection / self-uplink ---

    /// Declare (or withdraw, with `None`) this node's own uplink capability.
    /// The transport layer calls this when INTERNET-001 enters/leaves
    /// Connected (INTERNET.md §Mesh Fallback).
    pub fn set_self_capability(&mut self, capability: Option<GatewayCapability>) {
        self.self_capability = capability;
    }

    /// Is *this node* a gateway right now?
    pub fn is_gateway(&self) -> bool {
        self.self_capability.is_some()
    }

    /// Build the DISCO-001 capability tags this node should advertise when it
    /// is a gateway (`["gateway", "gateway:internet"]` etc.).
    pub fn advertised_tags(&self) -> Vec<String> {
        match &self.self_capability {
            Some(cap) => vec![
                GATEWAY_TAG.to_string(),
                format!("{}:{}", GATEWAY_TAG, cap.gateway_type.as_str()),
            ],
            None => Vec::new(),
        }
    }

    // --- Registry ---

    /// Adopt a gateway advertised by a neighbor (from its DISCO-001 capability
    /// bundle). Returns `Some(ev)` when adoption/update changes the world, so
    /// the caller can forward a `TopologyEvent::GatewayChanged`. The registry
    /// is bounded at [`MAX_GATEWAY_CANDIDATES`] — when full with a genuinely
    /// new candidate the oldest-confirmed gateway is evicted (redteam-hardened).
    pub fn adopt_from_neighbor(
        &mut self,
        node_id: PeerId,
        capability: GatewayCapability,
        hop_count: u8,
    ) -> Option<TopologyEvent> {
        let now = Instant::now();
        let gateway_type = capability.gateway_type.as_str().to_string();
        // Sanitize advertised floats at the boundary: NaN/double must not
        // poison scoring or de-rank a victim (REDTEAM-04).
        let capability = sanitize_capability(capability);
        // A fresh advertisement re-admits a previously-failed gateway (probation).
        self.health.handle_advertisement(node_id);

        // Read flags before touching the map mutably (borrow discipline).
        let is_known = self.gateways.contains_key(&node_id);
        let known_since = self
            .gateways
            .get(&node_id)
            .map(|g| g.discovered_at)
            .unwrap_or(now);
        let (known_capability, known_hops) = match self.gateways.get(&node_id).cloned() {
            Some(stored) => (Some(stored.capability), stored.hop_count),
            None => (None, 0),
        };
        if !is_known && self.gateways.len() >= MAX_GATEWAY_CANDIDATES {
            // MG-35: evict the weakest candidate, not the least-recently
            // confirmed — recency is exactly the metric a Sybil flood
            // maximizes for free (every fresh advertisement stamps
            // `last_confirmed: now`), so "oldest-confirmed" eviction let an
            // attacker's flood evict genuinely-established gateways.
            if let Some(victim) = self.weakest_gateway_for_eviction() {
                self.withdraw(&victim, "evicted: registry at capacity");
            }
        }
        // Preserve first-seen across refreshes.
        let discovered_at = known_since;
        let changed = match known_capability {
            Some(existing_cap) => existing_cap != capability || known_hops != hop_count,
            None => true,
        };
        let reliability = capability.reliability_score;
        self.gateways.insert(
            node_id,
            GatewayCandidate {
                node_id,
                capability,
                hop_count,
                discovered_at,
                last_confirmed: now,
            },
        );
        if changed {
            tracing::info!(
                event = event::GW_ADOPTED,
                gateway = %node_id.short(),
                gateway_type = %gateway_type,
                reliability = %reliability,
                "gateway candidate adopted"
            );
            let ev = TopologyEvent::GatewayChanged {
                gateway_id: node_id,
                gateway_type,
                available: true,
            };
            // MG-31: publish regardless of whether this call's own return
            // value is consumed — `reconcile` (the only internal caller
            // today) discards it, which used to mean the event never went
            // anywhere at all.
            self.topology_tx.send(ev.clone()).ok();
            Some(ev)
        } else {
            None
        }
    }

    /// MG-35: pick the weakest current candidate to make room for a new one
    /// — lowest `quality × health × confidence`, at a representative P4
    /// priority (eviction is a capacity decision, not a per-message one).
    /// Prefers evicting a candidate with **zero recorded successes** over
    /// any that has actually proven a delivery, so a flood of fresh
    /// advertisements cannot evict an established gateway merely by
    /// out-numbering it; only reaches into the proven set if every current
    /// candidate has proven itself. Deterministic tie-break by `PeerId`
    /// (REDTEAM-05 — never let `HashMap` iteration order leak).
    fn weakest_gateway_for_eviction(&self) -> Option<PeerId> {
        let now = Instant::now();
        let score = |g: &GatewayCandidate| -> f32 {
            let factor =
                self.health.score_factor(&g.node_id, now) * self.health.confidence_factor(&g.node_id);
            let hop_discount = HOP_DISCOUNT.powi(g.hop_count as i32);
            compute_gateway_quality(&g.capability, MessagePriority::P4) * factor * hop_discount
        };
        let unproven: Vec<&GatewayCandidate> = self
            .gateways
            .values()
            .filter(|g| self.health.confidence_factor(&g.node_id) <= MIN_CONFIDENCE)
            .collect();
        let pool: Vec<&GatewayCandidate> = if unproven.is_empty() {
            self.gateways.values().collect()
        } else {
            unproven
        };
        pool.into_iter()
            .min_by(|a, b| score(a).total_cmp(&score(b)).then_with(|| a.node_id.cmp(&b.node_id)))
            .map(|g| g.node_id)
    }

    /// Withdraw a gateway (peer lost, capability removed, capacity eviction,
    /// or this node's own uplink dropped). Returns the
    /// `GatewayChanged { available: false }` event. `reason` (MG-35) is
    /// logged verbatim — previously every withdrawal was mislabeled
    /// `"stale"`, including capacity evictions, making them
    /// indistinguishable from ordinary staleness in the logs.
    pub fn withdraw(&mut self, node_id: &PeerId, reason: &'static str) -> Option<TopologyEvent> {
        if let Some(removed) = self.gateways.remove(node_id) {
            tracing::info!(
                event = event::GW_WITHDRAWN,
                gateway = %node_id.short(),
                gateway_type = %removed.capability.gateway_type.as_str(),
                reason = %reason,
                "gateway candidate withdrawn"
            );
            let ev = TopologyEvent::GatewayChanged {
                gateway_id: *node_id,
                gateway_type: removed.capability.gateway_type.as_str().to_string(),
                available: false,
            };
            self.topology_tx.send(ev.clone()).ok();
            Some(ev)
        } else {
            None
        }
    }

    // --- Failure handling ---

    /// Record a delivery ACK timeout. Returns `true` when the gateway was
    /// newly excluded (caller should re-route and surface the change).
    ///
    /// MG-31: also publishes a `GatewayChanged { available: false }` event on
    /// that transition — previously failure produced no event at all, so a
    /// gateway going bad was invisible to anything not polling `is_failed`.
    pub fn record_ack_timeout(&mut self, gateway: PeerId) -> bool {
        let newly_excluded = self.health.record_ack_timeout(gateway);
        if newly_excluded {
            let gateway_type = self
                .gateways
                .get(&gateway)
                .map(|g| g.capability.gateway_type.as_str().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            self.topology_tx
                .send(TopologyEvent::GatewayChanged {
                    gateway_id: gateway,
                    gateway_type,
                    available: false,
                })
                .ok();
        }
        newly_excluded
    }

    pub fn record_success(&mut self, gateway: PeerId) {
        self.health.record_success(gateway);
    }

    pub fn is_failed(&self, gateway: &PeerId) -> bool {
        self.health.is_failed(gateway)
    }

    // --- Queries + selection ---

    pub fn known_gateways(&self) -> Vec<GatewayCandidate> {
        self.gateways.values().cloned().collect()
    }

    pub fn gateway(&self, node_id: &PeerId) -> Option<&GatewayCandidate> {
        self.gateways.get(node_id)
    }

    pub fn len(&self) -> usize {
        self.gateways.len()
    }

    pub fn is_empty(&self) -> bool {
        self.gateways.is_empty()
    }

    /// Deterministic selection for a message priority. Never returns a failed
    /// or probation-expired gateway; with no compatible candidate it returns
    /// `None` (mesh-only fallback — message stays in mesh/SCF, never dropped).
    ///
    /// `&mut self` (MG-33): a P3+ selection is sticky against a challenger
    /// that doesn't clearly beat the incumbent, and that incumbent has to
    /// live somewhere between calls.
    pub fn select(&mut self, msg_priority: MessagePriority) -> GatewaySelection {
        self.select_at(msg_priority, Instant::now())
    }

    fn select_at(&mut self, msg_priority: MessagePriority, now: Instant) -> GatewaySelection {
        let selection = self.select_inner(msg_priority, now);
        let label = match &selection {
            GatewaySelection::None { .. } => "None",
            GatewaySelection::All { .. } => "All",
            GatewaySelection::WithBackup { .. } => "WithBackup",
            GatewaySelection::Single { .. } => "Single",
        };
        tracing::info!(
            event = event::GW_SELECTED,
            priority = msg_priority.as_u8(),
            selection = label,
            gateways_known = self.gateways.len(),
            "gateway selection resolved"
        );
        selection
    }

    /// MG-33: a challenger must exceed the current incumbent by this margin
    /// before `select` actually switches — otherwise a link-quality flap at
    /// a score boundary re-elects a different gateway on alternating calls,
    /// fragmenting an in-flight multi-part transfer across two gateways with
    /// no shared session/NAT state.
    const SWITCH_MARGIN: f32 = 0.10;

    fn select_inner(&mut self, msg_priority: MessagePriority, now: Instant) -> GatewaySelection {
        let mut scored: Vec<(f32, PeerId)> = self
            .gateways
            .values()
            .map(|g| {
                // MG-25/27/29: health (is it currently trusted?) and
                // confidence (has it *proven* itself?) are independent axes
                // — a brand-new, never-failed gateway is `Healthy` (factor
                // 1.0) but should still be scaled down until it has actually
                // delivered something.
                let factor =
                    self.health.score_factor(&g.node_id, now) * self.health.confidence_factor(&g.node_id);
                // MG-34: hop distance previously had no effect on score at
                // all — always 0 today (multi-hop propagation is deferred),
                // but wiring the discount in now means a 2-hop gateway
                // won't silently tie a direct neighbor once multi-hop
                // lands. `HOP_DISCOUNT.powi(1)` ≈ 15% off per hop.
                let hop_discount = HOP_DISCOUNT.powi(g.hop_count as i32);
                (
                    compute_gateway_quality(&g.capability, msg_priority) * factor * hop_discount,
                    g.node_id,
                )
            })
            .filter(|(score, _)| score.is_finite() && *score > MIN_QUALITY_FLOOR)
            .collect();
        // MG-32: this node's own uplink is a candidate too — previously
        // `self_capability` fed only `is_gateway()`/`advertised_tags()`, so
        // a node holding the only internet uplink in the neighbourhood
        // still reported "no gateway available" for its own traffic. Not
        // subject to the health monitor (there is no ACK-timeout concept
        // for routing to yourself) or to the confidence ramp (a node always
        // knows its own uplink is real, unlike an advertised claim).
        if let (Some(cap), Some(local_id)) = (&self.self_capability, self.local_id) {
            let q = compute_gateway_quality(cap, msg_priority);
            if q.is_finite() && q > MIN_QUALITY_FLOOR {
                scored.push((q, local_id));
            }
        }
        // Deterministic total order: score desc, then PeerId asc for ties —
        // HashMap iteration order must never leak into selection (REDTEAM-05).
        scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        if scored.is_empty() {
            return GatewaySelection::None {
                reason: "No compatible gateway available".to_string(),
            };
        }

        match msg_priority {
            MessagePriority::P0 => GatewaySelection::All {
                gateways: scored.iter().map(|(_, id)| *id).collect(),
            },
            MessagePriority::P1 | MessagePriority::P2 => {
                if scored.len() >= 2 {
                    GatewaySelection::WithBackup {
                        primary: scored[0].1,
                        backup: scored[1].1,
                        primary_score: scored[0].0,
                    }
                } else {
                    GatewaySelection::Single {
                        gateway: scored[0].1,
                        score: scored[0].0,
                    }
                }
            }
            _ => {
                let mut chosen = scored[0];
                if let Some(&incumbent) = self.elected.get(&msg_priority) {
                    if let Some(&incumbent_scored) =
                        scored.iter().find(|(_, id)| *id == incumbent)
                    {
                        if chosen.0 < incumbent_scored.0 + Self::SWITCH_MARGIN {
                            chosen = incumbent_scored;
                        }
                    }
                }
                self.elected.insert(msg_priority, chosen.1);
                GatewaySelection::Single {
                    gateway: chosen.1,
                    score: chosen.0,
                }
            }
        }
    }

    /// Reconcile the registry against DISCO-001's neighbor table: adopt any
    /// neighbor whose capability bundle carries the `gateway` tag, and
    /// **withdraw** candidates that are no longer present, no longer tagged,
    /// or no longer linked up (diff-based — redteam-hardened so the registry
    /// cannot grow one-way). Read-only w.r.t. the table — no DISCO-001 edits.
    pub async fn reconcile(&mut self, table: &NeighborTable) {
        let neighbors = table.neighbors().await;
        let mut current: Vec<PeerId> = Vec::new();
        for nb in neighbors {
            let Ok(tags) = self.tags_of(&nb) else {
                continue;
            };
            if nb.state != crate::discovery::neighbor_table::NeighborState::LinkedUp {
                continue; // peer with no live link is not a usable gateway
            }
            if tags.iter().any(|t| t == GATEWAY_TAG) {
                current.push(nb.peer_id);
                let gw_type = if tags.iter().any(|t| t == GATEWAY_TAG_INTERNET) {
                    GatewayType::Internet
                } else if tags.iter().any(|t| t == GATEWAY_TAG_LORA) {
                    GatewayType::Lora
                } else if tags.iter().any(|t| t == GATEWAY_TAG_SATELLITE) {
                    GatewayType::Satellite
                } else {
                    GatewayType::Internet // bare `gateway` tag → internet default
                };
                let transport = nb
                    .links
                    .first()
                    .map(|l| l.transport.as_str().to_string())
                    .unwrap_or_else(|| "mesh".to_string());
                let mut cap = GatewayCapability::new(gw_type, transport);
                cap.reliability_score = match nb.links.first().map(|l| l.quality) {
                    Some(crate::message::LinkQuality::Excellent) => 0.95,
                    Some(crate::message::LinkQuality::Good) => 0.85,
                    Some(crate::message::LinkQuality::Fair) => 0.6,
                    Some(crate::message::LinkQuality::Poor) | None => 0.4,
                };
                let _ = self.adopt_from_neighbor(nb.peer_id, cap, 0);
            }
        }
        // Diff-based withdrawal: any registered gateway that is no longer a
        // tagged, linked-up neighbor leaves the registry (REDTEAM-02).
        let known: Vec<PeerId> = self.gateways.keys().copied().collect();
        for node_id in known {
            if !current.contains(&node_id) {
                self.withdraw(&node_id, "stale");
            }
        }
        let known: Vec<PeerId> = self.gateways.keys().copied().collect();
        self.health.prune(&known);
    }

    fn tags_of(&self, nb: &crate::discovery::neighbor_table::Neighbor) -> Result<Vec<String>, ()> {
        match nb.capabilities.as_ref() {
            Some(bundle) => Ok(bundle.capabilities.clone()),
            None => Err(()),
        }
    }
}

/// MG-36: reference monetary cost (currency units per KB) treated as
/// maximally expensive for `cost_factor` normalization. A judgment call,
/// not a measured figure — chosen so this crate's own worked-example
/// metered-cellular rate (₹0.50/KB, see `self_internet_gateway_from_transport_cost`)
/// lands mid-scale rather than reading as "essentially free" the way the
/// previous bare `/10.0` divisor did (`0.5/10.0 = 0.05`).
pub const REFERENCE_EXPENSIVE_COST_PER_KB: f32 = 1.0;

/// Build the self-uplink capability for an Internet transport that has just
/// become connected. Called by the transport/manager layer on
/// `TransportState::Connected` (INTERNET-001).
pub fn self_internet_gateway(
    transport_id: impl Into<String>,
    cost: crate::transport::TransportCost,
    caps: &crate::transport::TransportCapabilities,
) -> GatewayCapability {
    let mut cap = GatewayCapability::new(GatewayType::Internet, transport_id);
    // MG-36: use the transport's *live* available bandwidth (capped by the
    // datasheet ceiling) — the previous code took only the static
    // `typical_throughput_bps`, ignoring the live figure it was handed.
    cap.bandwidth_bps = cost.bandwidth_available_bps.min(caps.typical_throughput_bps);
    cap.latency_ms = caps.typical_latency_ms;
    cap.cost_factor =
        (cost.monetary_cost_per_kb as f32 / REFERENCE_EXPENSIVE_COST_PER_KB).clamp(0.0, 1.0);
    cap.current_load = cost.congestion_level.clamp(0.0, 1.0);
    // MG-36: no ACK-rate EWMA exists anywhere in this crate to feed a real
    // reliability estimate — this function is a pure mapping of one-shot
    // `cost`/`caps` snapshots, with no access to delivery history. Building
    // that tracker (and wiring it through the also-unwired INTERNET-001
    // transport layer — MG-30) is a separate, larger addition; kept as a
    // fixed placeholder, same value as before.
    cap.reliability_score = 0.9;
    // MG-36: run through the same plausibility/NaN sanitizer applied to
    // peer advertisements — harmless for a real transport's own figures,
    // and stops a `congestion_level: NaN` (MG-3's threat) from producing a
    // non-finite quality score for our own uplink too.
    sanitize_capability(cap)
}

/// Clamp/sanitize *every* field at the adoption boundary (REDTEAM-04 covered
/// only the float NaN/inf case; MG-27 extends this to plausibility for the
/// integer/enum fields too): an untrusted advertisement must not poison the
/// quality score, win selection on an implausible claim, or bypass the
/// priority-compatibility gate.
fn sanitize_capability(mut cap: GatewayCapability) -> GatewayCapability {
    cap.reliability_score = sanitize_f32(cap.reliability_score, 0.6);
    cap.cost_factor = sanitize_f32(cap.cost_factor, 0.5);
    cap.current_load = sanitize_f32(cap.current_load, 0.5);
    cap.duty_cycle_remaining = sanitize_f32(cap.duty_cycle_remaining, 1.0);
    // MG-27: `max_priority` is a *gate*, not a preference — it must be
    // derived from the (also-advertised, but at least internally
    // consistent) gateway type rather than trusted verbatim, or an
    // advertiser can claim "I serve P7" on a link that structurally cannot.
    cap.max_priority = cap.gateway_type.default_max_priority();
    cap.bandwidth_bps = cap.bandwidth_bps.min(MAX_EXPECTED_BPS);
    cap.latency_ms = cap.latency_ms.max(MIN_PLAUSIBLE_LATENCY_MS);
    cap.queue_depth = cap.queue_depth.min(1_000);
    cap
}

/// `0.0..=1.0` plus a NaN/default fallback.
fn sanitize_f32(v: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn pid(id: u8) -> PeerId {
        let mut b = [0u8; 32];
        b[0] = id;
        PeerId::from_bytes(b)
    }

    /// Like `pid` but distinct for every value beyond `u8::MAX` — needed to
    /// exercise `MAX_QUARANTINE_ENTRIES` (256), which `pid`'s single byte
    /// cannot address without wrapping.
    fn pid_wide(id: u32) -> PeerId {
        let mut b = [0u8; 32];
        b[..4].copy_from_slice(&id.to_le_bytes());
        PeerId::from_bytes(b)
    }

    fn gw_cap(gw_type: GatewayType) -> GatewayCapability {
        GatewayCapability::new(gw_type, "internet-0")
    }

    fn rich_cap(
        gw_type: GatewayType,
        bandwidth_bps: u64,
        latency_ms: u32,
        reliability: f32,
        cost: f32,
    ) -> GatewayCapability {
        GatewayCapability {
            gateway_type: gw_type,
            transport_id: "t".into(),
            bandwidth_bps,
            latency_ms,
            reliability_score: reliability,
            cost_factor: cost,
            current_load: 0.1,
            queue_depth: 0,
            duty_cycle_remaining: 1.0,
            max_priority: gw_type.default_max_priority(),
        }
    }

    #[test]
    fn serves_priority_follows_type_defaults() {
        // Internet/USB: everything.
        assert!(gw_cap(GatewayType::Internet).serves_priority(MessagePriority::P7));
        assert!(gw_cap(GatewayType::UsbEthernet).serves_priority(MessagePriority::P7));
        // LoRa: P0–P3.
        let lora = gw_cap(GatewayType::Lora);
        assert!(lora.serves_priority(MessagePriority::P0));
        assert!(lora.serves_priority(MessagePriority::P3));
        assert!(!lora.serves_priority(MessagePriority::P4));
        // Satellite: P0–P2.
        let sat = gw_cap(GatewayType::Satellite);
        assert!(sat.serves_priority(MessagePriority::P2));
        assert!(!sat.serves_priority(MessagePriority::P3));
    }

    #[test]
    fn priority_incompatible_gateway_scores_zero() {
        let lora = rich_cap(GatewayType::Lora, 100_000, 20, 1.0, 0.0);
        assert_eq!(compute_gateway_quality(&lora, MessagePriority::P7), 0.0);
        // Compatible → positive.
        assert!(compute_gateway_quality(&lora, MessagePriority::P3) > 0.0);
    }

    #[test]
    fn quality_score_ranks_expected_order() {
        let fast = rich_cap(GatewayType::Internet, 100_000_000, 30, 0.95, 0.05);
        let slow = rich_cap(GatewayType::Internet, 1_000_000, 200, 0.8, 0.05);
        let pricey = rich_cap(GatewayType::Internet, 100_000_000, 30, 0.95, 0.9);
        let sq = compute_gateway_quality(&slow, MessagePriority::P4);
        let fq = compute_gateway_quality(&fast, MessagePriority::P4);
        let pq = compute_gateway_quality(&pricey, MessagePriority::P4);
        assert!(fq > sq, "fast must score higher than slow: {fq} vs {sq}");
        assert!(fq > pq, "free must score higher than pricey: {fq} vs {pq}");
        assert!(fq > 0.7 && fq <= 1.0);
    }

    #[test]
    fn health_thresholds_and_recovery() {
        let mut h = GatewayHealthMonitor::new();
        let g = pid(9);
        let now = Instant::now();
        assert_eq!(h.score_factor(&g, now), 1.0);
        // 1..2 timeouts: still healthy.
        assert!(!h.record_ack_timeout(g));
        assert!(!h.record_ack_timeout(g));
        // 3rd timeout: failed, excluded.
        assert!(h.record_ack_timeout(g));
        assert!(h.is_failed(&g));
        assert_eq!(h.score_factor(&g, now), 0.0);
        // Fresh advertisement → probation with discount.
        h.handle_advertisement(g);
        assert!(!h.is_failed(&g));
        assert!(h.is_probation(&g));
        assert_eq!(h.score_factor(&g, now), PROBATION_SCORE_FACTOR);
        // ACK success → full trust, probation cleared.
        h.record_success(g);
        assert!(!h.is_probation(&g));
        assert_eq!(h.score_factor(&g, now), 1.0);
    }

    #[test]
    fn mg28_probation_never_auto_expires_to_full_score_without_a_success() {
        // Inverted from the old (buggy) `health_probation_expires_to_full_score`:
        // elapsing RECOVERY_WINDOW must NOT by itself restore full trust — a
        // re-advertising blackhole that has delivered nothing stays
        // discounted no matter how long it waits.
        let mut h = GatewayHealthMonitor::new();
        let g = pid(10);
        for _ in 0..FAILURE_THRESHOLD {
            let _ = h.record_ack_timeout(g);
        }
        h.handle_advertisement(g);
        let t0 = Instant::now();
        assert_eq!(h.score_factor(&g, t0), PROBATION_SCORE_FACTOR);
        let long_after_window = t0 + RECOVERY_WINDOW * 100 + Duration::from_secs(1);
        assert_eq!(
            h.score_factor(&g, long_after_window),
            PROBATION_SCORE_FACTOR,
            "elapsed time alone must never restore full trust"
        );
        // Only a genuine successful delivery restores it.
        h.record_success(g);
        assert_eq!(h.score_factor(&g, long_after_window), 1.0);
    }

    #[test]
    fn mg26_prune_preserves_failure_strikes_across_a_withdrawal_cycle() {
        // The exact scenario MG-26 describes: fail a gateway out, let it
        // drop from the known set for one reconcile (prune), then have it
        // reappear — the old code erased `states`/`failure_counters` on
        // prune, so this re-admitted as Healthy with zero strikes.
        let mut h = GatewayHealthMonitor::new();
        let g = pid(20);
        for _ in 0..FAILURE_THRESHOLD {
            let _ = h.record_ack_timeout(g);
        }
        assert!(h.is_failed(&g));
        // Gateway drops off the neighbor table — reconcile prunes with an
        // empty known set.
        h.prune(&[]);
        assert!(
            h.is_failed(&g),
            "a Failed record must survive being pruned from the known set"
        );
        assert_eq!(h.score_factor(&g, Instant::now()), 0.0);
        // It re-advertises — re-admitted to probation, not Healthy.
        h.handle_advertisement(g);
        assert!(h.is_probation(&g));
        assert!(!h.is_failed(&g));
    }

    #[test]
    fn mg26_quarantine_is_bounded() {
        // An attacker cycling through fresh PeerIds, each failed out once,
        // must not grow the health monitor's bookkeeping without limit now
        // that Failed/HardFailed records survive pruning.
        let mut h = GatewayHealthMonitor::new();
        for i in 0..(MAX_QUARANTINE_ENTRIES as u32 + 50) {
            let g = pid_wide(i);
            for _ in 0..FAILURE_THRESHOLD {
                let _ = h.record_ack_timeout(g);
            }
            h.prune(&[]);
        }
        assert!(
            h.quarantine_len() <= MAX_QUARANTINE_ENTRIES,
            "quarantine must stay bounded, got {}",
            h.quarantine_len()
        );
    }

    #[test]
    fn mg25_mg29_fresh_gateway_does_not_outscore_a_proven_one() {
        // MG-25's own trigger scenario: an attacker's freshly-adopted,
        // self-tagged gateway with a strong link but zero delivery history
        // must not simply win selection outright over an established
        // gateway with a real (if less flattering) track record.
        let mut m = GatewayManager::new();
        let attacker = pid(50);
        let established = pid(51);
        // Attacker: excellent link, nothing else measured — the exact shape
        // reconcile() produces for a bare `gateway` tag.
        let mut attacker_cap = GatewayCapability::new(GatewayType::Internet, "sim");
        attacker_cap.reliability_score = 0.95;
        let _ = m.adopt_from_neighbor(attacker, attacker_cap, 0);
        // Established: modest but real measured capability, with a proven
        // delivery record.
        let established_cap = rich_cap(GatewayType::Internet, 20_000_000, 80, 0.85, 0.1);
        let _ = m.adopt_from_neighbor(established, established_cap, 0);
        for _ in 0..CONFIDENCE_SATURATION_SAMPLES {
            m.record_success(established);
        }
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => {
                assert_eq!(
                    gateway, established,
                    "an unmeasured, freshly-adopted gateway must not outrank a proven one"
                );
            }
            other => panic!("expected single, got {other:?}"),
        }
    }

    #[test]
    fn mg27_sanitize_reasserts_priority_gate_and_clamps_plausibility() {
        // An advertiser cannot claim a priority gate above what its own
        // gateway_type supports, nor implausible bandwidth/latency/queue
        // values.
        let mut cap = GatewayCapability::new(GatewayType::Satellite, "sim");
        cap.max_priority = MessagePriority::P7; // satellite really serves only P0-P2
        cap.bandwidth_bps = u64::MAX;
        cap.latency_ms = 0;
        cap.queue_depth = u32::MAX;
        let mut m = GatewayManager::new();
        let node = pid(60);
        let _ = m.adopt_from_neighbor(node, cap, 0);
        let stored = &m.gateway(&node).expect("adopted").capability;
        assert_eq!(
            stored.max_priority,
            GatewayType::Satellite.default_max_priority(),
            "max_priority must be re-derived from gateway_type, not trusted verbatim"
        );
        assert!(!stored.serves_priority(MessagePriority::P7));
        assert_eq!(stored.bandwidth_bps, MAX_EXPECTED_BPS);
        assert_eq!(stored.latency_ms, MIN_PLAUSIBLE_LATENCY_MS);
        assert_eq!(stored.queue_depth, 1_000);
    }

    #[test]
    fn mg31_adopt_withdraw_and_failure_all_publish_topology_events() {
        let mut m = GatewayManager::new();
        let mut rx = m.topology_events();
        let g = pid(30);
        let _ = m.adopt_from_neighbor(
            g,
            rich_cap(GatewayType::Internet, 10_000_000, 100, 0.8, 0.1),
            0,
        );
        match rx.try_recv() {
            Ok(TopologyEvent::GatewayChanged {
                gateway_id,
                available: true,
                ..
            }) => assert_eq!(gateway_id, g),
            other => panic!("expected adopt event, got {other:?}"),
        }
        for _ in 0..FAILURE_THRESHOLD {
            let _ = m.record_ack_timeout(g);
        }
        match rx.try_recv() {
            Ok(TopologyEvent::GatewayChanged {
                gateway_id,
                available: false,
                ..
            }) => assert_eq!(gateway_id, g),
            other => panic!("expected failure event, got {other:?}"),
        }
        let _ = m.withdraw(&g, "test");
        match rx.try_recv() {
            Ok(TopologyEvent::GatewayChanged {
                gateway_id,
                available: false,
                ..
            }) => assert_eq!(gateway_id, g),
            other => panic!("expected withdraw event, got {other:?}"),
        }
    }

    #[test]
    fn mg32_self_uplink_is_a_selection_candidate() {
        let mut m = GatewayManager::new().with_local_id(pid(1));
        // No peer gateways at all — only this node's own uplink.
        m.set_self_capability(Some(rich_cap(
            GatewayType::Internet,
            100_000_000,
            10,
            0.99,
            0.0,
        )));
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => assert_eq!(gateway, pid(1)),
            other => panic!("self uplink must be selectable, got {other:?}"),
        }
    }

    #[test]
    fn mg32_without_local_id_self_is_never_a_candidate() {
        // The opt-in builder must not change behavior for the 13 existing
        // `new()` call sites that never call `with_local_id`.
        let mut m = GatewayManager::new();
        m.set_self_capability(Some(rich_cap(
            GatewayType::Internet,
            100_000_000,
            10,
            0.99,
            0.0,
        )));
        assert!(matches!(
            m.select(MessagePriority::P4),
            GatewaySelection::None { .. }
        ));
    }

    #[test]
    fn mg33_hysteresis_holds_the_incumbent_across_a_small_score_flap() {
        let mut m = GatewayManager::new();
        let a = pid(40);
        let b = pid(41);
        let _ = m.adopt_from_neighbor(
            a,
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.90, 0.05),
            0,
        );
        let _ = m.adopt_from_neighbor(
            b,
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.85, 0.05),
            0,
        );
        // a clearly wins first — becomes the incumbent.
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => assert_eq!(gateway, a),
            other => panic!("expected single, got {other:?}"),
        }
        // a's link quality flaps down slightly — just enough that b would
        // narrowly out-score it, but well within SWITCH_MARGIN.
        let _ = m.adopt_from_neighbor(
            a,
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.83, 0.05),
            0,
        );
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => {
                assert_eq!(gateway, a, "must not switch gateways on a small score flap")
            }
            other => panic!("expected single, got {other:?}"),
        }
    }

    #[test]
    fn mg33_hysteresis_does_not_block_a_real_regression() {
        // A genuine, well-beyond-the-margin drop must still fail over —
        // hysteresis dampens noise, it must not paper over a real problem.
        let mut m = GatewayManager::new();
        let a = pid(42);
        let b = pid(43);
        let _ = m.adopt_from_neighbor(
            a,
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.95, 0.05),
            0,
        );
        let _ = m.adopt_from_neighbor(
            b,
            rich_cap(GatewayType::Internet, 20_000_000, 80, 0.85, 0.05),
            0,
        );
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => assert_eq!(gateway, a),
            other => panic!("expected single, got {other:?}"),
        }
        for _ in 0..FAILURE_THRESHOLD {
            let _ = m.record_ack_timeout(a);
        }
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => {
                assert_eq!(gateway, b, "a real failure must still fail over")
            }
            other => panic!("expected single, got {other:?}"),
        }
    }

    #[test]
    fn mg34_hop_count_penalizes_the_score() {
        let mut m = GatewayManager::new();
        let direct = pid(70);
        let two_hops = pid(71);
        // Identical capability, differing only in hop distance.
        let _ = m.adopt_from_neighbor(
            direct,
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.85, 0.05),
            0,
        );
        let _ = m.adopt_from_neighbor(
            two_hops,
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.85, 0.05),
            2,
        );
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => assert_eq!(
                gateway, direct,
                "a closer gateway must win over an identical-capability farther one"
            ),
            other => panic!("expected single, got {other:?}"),
        }
    }

    #[test]
    fn mg35_capacity_eviction_prefers_unproven_over_established() {
        // The exact scenario MG-35 describes: a flood of fresh (unproven)
        // candidates must not be able to evict a genuinely-established
        // gateway just by out-numbering it at the capacity bound.
        let mut m = GatewayManager::new();
        for i in 0..MAX_GATEWAY_CANDIDATES as u32 {
            let _ = m.adopt_from_neighbor(
                pid_wide(i),
                rich_cap(GatewayType::Internet, 10_000_000, 100, 0.5, 0.1),
                0,
            );
        }
        let established = pid_wide(0);
        m.record_success(established);
        assert_eq!(m.len(), MAX_GATEWAY_CANDIDATES);

        // Registry is full; one more (also unproven) candidate arrives.
        let newcomer = pid_wide(MAX_GATEWAY_CANDIDATES as u32 + 1);
        let _ = m.adopt_from_neighbor(
            newcomer,
            rich_cap(GatewayType::Internet, 10_000_000, 100, 0.5, 0.1),
            0,
        );
        assert_eq!(m.len(), MAX_GATEWAY_CANDIDATES, "registry must stay bounded");
        assert!(
            m.gateway(&established).is_some(),
            "a proven gateway must not be evicted while unproven candidates exist"
        );
    }

    #[test]
    fn redteam_rogue_readmission_escalates_to_hard_failed() {
        // A blackhole that keeps re-advertising must not loop failed→probation
        // forever: repeated cycles escalate to HardFailed (REDTEAM-01).
        let mut h = GatewayHealthMonitor::new();
        let rogue = pid(77);
        // First failure: Healthy → Failed (fresh healthy gateway).
        for _ in 0..FAILURE_THRESHOLD {
            let _ = h.record_ack_timeout(rogue);
        }
        assert!(h.is_failed(&rogue));
        // Fail out of probation repeatedly → each exit increments re-admission
        // strikes; HARD_FAIL_STRIKES exits trips HardFailed.
        for _ in 0..HARD_FAIL_STRIKES {
            h.handle_advertisement(rogue); // Failed → Probation
            for _ in 0..FAILURE_THRESHOLD {
                let _ = h.record_ack_timeout(rogue); // fail out of probation
            }
        }
        // After HARD_FAIL_STRIKES failed→probation exits, hard-excluded.
        assert!(h.is_failed(&rogue));
        assert_eq!(h.score_factor(&rogue, Instant::now()), 0.0);
        // Re-advertising cannot re-admit a hard-failed gateway.
        h.handle_advertisement(rogue);
        assert!(h.is_failed(&rogue));
        assert_eq!(h.score_factor(&rogue, Instant::now()), 0.0);
        // Only an explicit success (probe pass) restores it.
        h.record_success(rogue);
        assert_eq!(h.score_factor(&rogue, Instant::now()), 1.0);
    }

    #[test]
    fn redteam_advertisement_does_not_erase_failure_strikes() {
        // Even within the first probation cycle, the failure counter is not
        // reset by a re-advertisement — one more timeout re-excludes (and the
        // next cycle escalates).
        let mut h = GatewayHealthMonitor::new();
        let g = pid(78);
        for _ in 0..FAILURE_THRESHOLD {
            let _ = h.record_ack_timeout(g);
        }
        h.handle_advertisement(g); // probation
        assert!(h.is_probation(&g));
        // Re-advertisement alone must not reset the counter to 0.
        h.handle_advertisement(g);
        // 1 more timeout while in probation → fails again (strike 1).
        let _ = h.record_ack_timeout(g);
        assert!(h.is_failed(&g));
    }

    #[test]
    fn redteam_nan_fields_sanitized_and_scored_finite() {
        // NaN in advertised fields must not poison/blackhole a gateway
        // (REDTEAM-04): sanitize at boundary → finite score.
        let mut m = GatewayManager::new();
        let mut cap = rich_cap(GatewayType::Internet, 50_000_000, 40, f32::NAN, f32::NAN);
        cap.reliability_score = f32::NAN;
        cap.current_load = f32::INFINITY;
        let _ = m.adopt_from_neighbor(pid(5), cap, 0);
        let s = m.select(MessagePriority::P4);
        // Still selected (sanitized), with a finite positive score.
        match s {
            GatewaySelection::Single { score, .. } => {
                assert!(score.is_finite());
                assert!(score > 0.0);
            }
            other => panic!("sanitized gateway must be selectable, got {other:?}"),
        }
    }

    #[test]
    fn redteam_selection_tie_break_is_deterministic() {
        // Two identical-score gateways must sort deterministically by PeerId,
        // not HashMap iteration order (REDTEAM-05).
        let mut m = GatewayManager::new();
        let a = pid(1);
        let b = pid(2);
        let cap_a = rich_cap(GatewayType::Internet, 10_000_000, 100, 0.8, 0.1);
        let cap_b = rich_cap(GatewayType::Internet, 10_000_000, 100, 0.8, 0.1);
        let _ = m.adopt_from_neighbor(a, cap_a, 0);
        let _ = m.adopt_from_neighbor(b, cap_b, 0);
        for _ in 0..10 {
            match m.select(MessagePriority::P4) {
                GatewaySelection::Single { gateway, .. } => {
                    assert_eq!(gateway, a, "lower PeerId must win ties consistently")
                }
                other => panic!("expected single, got {other:?}"),
            }
        }
    }

    #[test]
    fn redteam_registry_capped_at_max_candidates() {
        let mut m = GatewayManager::new();
        for i in 0..(MAX_GATEWAY_CANDIDATES + 10) as u8 {
            let _ = m.adopt_from_neighbor(
                pid(i),
                rich_cap(GatewayType::Internet, 10_000_000, 100, 0.8, 0.1),
                0,
            );
        }
        assert!(
            m.len() <= MAX_GATEWAY_CANDIDATES,
            "registry must be bounded"
        );
    }

    #[test]
    fn selection_matrix_single_backup_all() {
        let mut m = GatewayManager::new();
        let _ = m.adopt_from_neighbor(
            pid(1),
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.95, 0.05),
            0,
        );
        let _ = m.adopt_from_neighbor(
            pid(2),
            rich_cap(GatewayType::Lora, 1_000_000, 1000, 0.9, 0.0),
            0,
        );
        let _ = m.adopt_from_neighbor(
            pid(3),
            rich_cap(GatewayType::Internet, 10_000_000, 100, 0.7, 0.1),
            0,
        );

        // P4 → single best (pid 1: fastest + most reliable).
        let s = m.select(MessagePriority::P4);
        match s {
            GatewaySelection::Single { gateway, .. } => assert_eq!(gateway, pid(1)),
            other => panic!("expected single, got {other:?}"),
        }

        // P2 → primary + backup.
        match m.select(MessagePriority::P2) {
            GatewaySelection::WithBackup {
                primary, backup, ..
            } => {
                assert_eq!(primary, pid(1));
                assert_eq!(backup, pid(2), "lora (free, serves P2) must be backup");
            }
            other => panic!("expected with-backup, got {other:?}"),
        }

        // P0 → all eligible (LoRa serves P0 too).
        match m.select(MessagePriority::P0) {
            GatewaySelection::All { gateways } => {
                assert_eq!(gateways.len(), 3);
            }
            other => panic!("expected all, got {other:?}"),
        }
    }

    #[test]
    fn no_candidates_returns_none_and_mesh_fallback() {
        let mut m = GatewayManager::new();
        let s = m.select(MessagePriority::P4);
        assert!(matches!(s, GatewaySelection::None { .. }));
        assert!(s.chosen_gateways().is_empty());
    }

    #[test]
    fn priority_incompatible_candidates_skipped() {
        let mut m = GatewayManager::new();
        let _ = m.adopt_from_neighbor(
            pid(1),
            rich_cap(GatewayType::Lora, 1_000_000, 500, 0.9, 0.0),
            0,
        );
        // P7 not served by LoRa → None (no internet gateway).
        assert!(matches!(
            m.select(MessagePriority::P7),
            GatewaySelection::None { .. }
        ));
    }

    #[test]
    fn failover_picks_backup_after_timeouts() {
        let mut m = GatewayManager::new();
        let _ = m.adopt_from_neighbor(
            pid(1),
            rich_cap(GatewayType::Internet, 50_000_000, 40, 0.95, 0.05),
            0,
        );
        let _ = m.adopt_from_neighbor(
            pid(2),
            rich_cap(GatewayType::Internet, 20_000_000, 80, 0.85, 0.05),
            0,
        );
        // P4 initially picks pid(1).
        assert!(
            matches!(m.select(MessagePriority::P4), GatewaySelection::Single { gateway, .. } if gateway == pid(1))
        );
        // Primary fails (3 consecutive timeouts).
        let _ = m.record_ack_timeout(pid(1));
        let _ = m.record_ack_timeout(pid(1));
        let _ = m.record_ack_timeout(pid(1));
        // Selection fails over to backup.
        match m.select(MessagePriority::P4) {
            GatewaySelection::Single { gateway, .. } => assert_eq!(gateway, pid(2)),
            other => panic!("expected failover to backup, got {other:?}"),
        }
        // Record a success on pid(1) → it recovers; still best → primary again.
        m.record_success(pid(1));
        if let GatewaySelection::Single { gateway, .. } = m.select(MessagePriority::P4) {
            assert_eq!(gateway, pid(1));
        } else {
            panic!("recovered primary must win again");
        }
    }

    #[test]
    fn withdraw_returns_event_and_removes_candidate() {
        let mut m = GatewayManager::new();
        let _ = m.adopt_from_neighbor(
            pid(4),
            rich_cap(GatewayType::Internet, 10_000_000, 50, 0.9, 0.0),
            0,
        );
        assert_eq!(m.len(), 1);
        let ev = m.withdraw(&pid(4), "test").expect("event");
        assert!(matches!(
            ev,
            TopologyEvent::GatewayChanged {
                available: false,
                ..
            }
        ));
        assert_eq!(m.len(), 0);
    }

    #[test]
    fn self_uplink_connected_marks_gateway() {
        let mut m = GatewayManager::new();
        assert!(!m.is_gateway());
        // INTERNET-001 reports Connected → self is a gateway.
        m.set_self_capability(Some(gw_cap(GatewayType::Internet)));
        assert!(m.is_gateway());
        assert!(m.advertised_tags().contains(&"gateway".to_string()));
        assert!(m
            .advertised_tags()
            .contains(&"gateway:internet".to_string()));
        // Take interval down → not a gateway anymore.
        m.set_self_capability(None);
        assert!(!m.is_gateway());
        assert!(m.advertised_tags().is_empty());
    }

    #[test]
    fn self_internet_gateway_from_transport_cost() {
        use crate::transport::{TransportCapabilities, TransportCost, TransportCostClass};
        let caps = TransportCapabilities {
            max_message_size: 1 << 20,
            supports_broadcast: false,
            supports_unicast: true,
            supports_multicast: false,
            range_m_min: 0,
            range_m_max: 0,
            range_m_typical: 0,
            typical_throughput_bps: 100_000_000,
            typical_latency_ms: 50,
            requires_infrastructure: true,
            supports_background_android: true,
            supports_background_ios: true,
            requires_special_hardware: false,
            cost_class: TransportCostClass::Metered,
            regulatory_band: None,
        };
        let cost = TransportCost {
            estimated_battery_ma: 50.0,
            monetary_cost_per_kb: 0.5,
            bandwidth_available_bps: 100_000_000,
            congestion_level: 0.1,
        };
        let cap = self_internet_gateway("internet-0", cost, &caps);
        assert_eq!(cap.gateway_type, GatewayType::Internet);
        assert_eq!(cap.bandwidth_bps, 100_000_000);
        assert!(cap.serves_priority(MessagePriority::P7));
        // MG-36: ₹0.50/KB must read as noticeably expensive, not as
        // "essentially free" (the old `/10.0` divisor gave 0.05 here).
        assert!(
            cap.cost_factor > 0.3,
            "got {} — a real per-KB charge must not look nearly free",
            cap.cost_factor
        );
    }

    #[test]
    fn mg36_uses_live_bandwidth_capped_by_the_datasheet_ceiling() {
        use crate::transport::{TransportCapabilities, TransportCost, TransportCostClass};
        let caps = TransportCapabilities {
            max_message_size: 1 << 20,
            supports_broadcast: false,
            supports_unicast: true,
            supports_multicast: false,
            range_m_min: 0,
            range_m_max: 0,
            range_m_typical: 0,
            typical_throughput_bps: 100_000_000, // datasheet ceiling
            typical_latency_ms: 50,
            requires_infrastructure: true,
            supports_background_android: true,
            supports_background_ios: true,
            requires_special_hardware: false,
            cost_class: TransportCostClass::Metered,
            regulatory_band: None,
        };
        // Live figure well below the datasheet ceiling — a congested link.
        let congested = TransportCost {
            estimated_battery_ma: 50.0,
            monetary_cost_per_kb: 0.1,
            bandwidth_available_bps: 5_000_000,
            congestion_level: 0.8,
        };
        let cap = self_internet_gateway("internet-0", congested, &caps);
        assert_eq!(
            cap.bandwidth_bps, 5_000_000,
            "must reflect the live available bandwidth, not the static datasheet figure"
        );

        // Live figure *above* the datasheet ceiling — must still be capped.
        let implausible = TransportCost {
            estimated_battery_ma: 50.0,
            monetary_cost_per_kb: 0.1,
            bandwidth_available_bps: 1_000_000_000,
            congestion_level: 0.1,
        };
        let cap2 = self_internet_gateway("internet-0", implausible, &caps);
        assert_eq!(cap2.bandwidth_bps, 100_000_000, "must be capped at the datasheet ceiling");
    }

    // --- M7 integration: DISCO-001 capability tags reconcile into GW-001 ---

    #[tokio::test]
    async fn reconcile_adopts_gateway_tagged_neighbor() {
        use crate::discovery::neighbor_table::NeighborTable;
        use crate::message::PeerInfo;
        use crate::transport::TransportId;

        let table = NeighborTable::new(Duration::from_secs(60));
        // A neighbor advertises gateway capability via the DISCO-001 handshake.
        let neighbor = pid(2);
        let mut bundle = crate::discovery::handshake::CapabilityBundle::new(neighbor);
        bundle.capabilities = vec!["gateway".into(), "gateway:internet".into()];
        bundle.transports = vec!["sim-alice".into()];
        table
            .upsert(
                &PeerInfo {
                    peer_id: neighbor,
                    addresses: vec![],
                    transport_addresses: vec![],
                    last_seen: None,
                },
                &TransportId::from("sim-alice"),
                crate::message::LinkQuality::Good,
            )
            .await;
        table.set_capabilities(&neighbor, bundle).await;
        // A non-gateway neighbor (relay only) must NOT be adopted.
        let other = pid(3);
        let mut b2 = crate::discovery::handshake::CapabilityBundle::new(other);
        b2.capabilities = vec!["relay".into()];
        table
            .upsert(
                &PeerInfo {
                    peer_id: other,
                    addresses: vec![],
                    transport_addresses: vec![],
                    last_seen: None,
                },
                &TransportId::from("sim"),
                crate::message::LinkQuality::Good,
            )
            .await;
        table.set_capabilities(&other, b2).await;

        // GW-001 reconciles read-only against the neighbor table.
        let mut gw = GatewayManager::new();
        gw.reconcile(&table).await;
        assert_eq!(gw.len(), 1, "only gateway-tagged neighbors are adopted");
        let cand = gw.gateway(&neighbor).expect("candidate");
        assert_eq!(cand.capability.gateway_type, GatewayType::Internet);
        assert_eq!(cand.hop_count, 0);

        // Diff-based: when the gateway neighbor is no longer in the table
        // (or no longer tagged/linked), it is withdrawn (REDTEAM-02).
        table
            .mark_down(&neighbor, &TransportId::from("sim-alice"))
            .await;
        gw.reconcile(&table).await;
        assert_eq!(gw.len(), 0, "stale gateway must be withdrawn");
    }

    #[tokio::test]
    async fn reconcile_withdraws_stale_and_keeps_healthy() {
        use crate::discovery::neighbor_table::NeighborTable;
        use crate::message::PeerInfo;
        use crate::transport::TransportId;

        let table = NeighborTable::new(Duration::from_secs(60));
        let gw1 = pid(11);
        let mut b1 = crate::discovery::handshake::CapabilityBundle::new(gw1);
        b1.capabilities = vec!["gateway".into(), "gateway:internet".into()];
        table
            .upsert(
                &PeerInfo {
                    peer_id: gw1,
                    addresses: vec![],
                    transport_addresses: vec![],
                    last_seen: None,
                },
                &TransportId::from("sim-a"),
                crate::message::LinkQuality::Good,
            )
            .await;
        table.set_capabilities(&gw1, b1).await;
        let gw2 = pid(12);
        let mut b2 = crate::discovery::handshake::CapabilityBundle::new(gw2);
        b2.capabilities = vec!["gateway".into(), "gateway:lora".into()];
        table
            .upsert(
                &PeerInfo {
                    peer_id: gw2,
                    addresses: vec![],
                    transport_addresses: vec![],
                    last_seen: None,
                },
                &TransportId::from("sim-b"),
                crate::message::LinkQuality::Good,
            )
            .await;
        table.set_capabilities(&gw2, b2).await;

        let mut gw = GatewayManager::new();
        gw.reconcile(&table).await;
        assert_eq!(gw.len(), 2);
        assert_eq!(
            gw.gateway(&gw2).unwrap().capability.gateway_type,
            GatewayType::Lora
        );
        // gw1 stops advertising / drops off → only gw2 remains.
        table.mark_down(&gw1, &TransportId::from("sim-a")).await;
        gw.reconcile(&table).await;
        assert_eq!(gw.len(), 1);
        assert!(gw.gateway(&gw1).is_none());
        assert!(gw.gateway(&gw2).is_some());
    }
}
