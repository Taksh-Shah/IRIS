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

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::discovery::neighbor_table::NeighborTable;
use crate::message::MessagePriority;
use crate::message::PeerId;
use crate::observability::event;
use crate::transport::TopologyEvent;

/// Discount applied to a gateway on the failed→probation transition.
pub const PROBATION_SCORE_FACTOR: f32 = 0.5;
/// Failure threshold: consecutive ACK timeouts that mark a gateway failed.
pub const FAILURE_THRESHOLD: u32 = 3;
/// Window after which a probation gateway may be re-admitted to full score.
pub const RECOVERY_WINDOW: Duration = Duration::from_secs(300);
/// Upper bound on tracked gateway candidates (eviction of oldest on overflow).
pub const MAX_GATEWAY_CANDIDATES: usize = 64;
/// Gateways scoring below this floor are excluded from selection (NaN/empty).
pub const MIN_QUALITY_FLOOR: f32 = 0.01;

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
            bandwidth_bps: 0,
            latency_ms: 0,
            reliability_score: 1.0,
            cost_factor: 0.0,
            current_load: 0.0,
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
    /// Hop distance: 0 = direct neighbor advertisement.
    pub hop_count: u8,
    /// Intermediate hops to reach the gateway via mesh (empty when direct).
    pub path: Vec<PeerId>,
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
#[derive(Debug, Clone)]
pub struct GatewayHealthMonitor {
    states: HashMap<PeerId, GatewayHealthState>,
    failure_counters: HashMap<PeerId, u32>,
    re_admission_strikes: HashMap<PeerId, u32>,
    probation_until: HashMap<PeerId, Instant>,
    last_success: HashMap<PeerId, Instant>,
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
            last_success: HashMap::new(),
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
        self.last_success.insert(gateway, Instant::now());
        tracing::debug!(
            event = event::GW_HEALTH_TRANSITION,
            gateway = %gateway.short(),
            to = "Healthy",
            "gateway health restored"
        );
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

    /// Score multiplier: 1.0 healthy (or expired probation), 0.5 in an active
    /// probation window, 0.0 failed/hard-failed. `now` injectable for tests.
    pub fn score_factor(&self, gateway: &PeerId, now: Instant) -> f32 {
        match self.states.get(gateway) {
            Some(GatewayHealthState::Failed) | Some(GatewayHealthState::HardFailed) => 0.0,
            Some(GatewayHealthState::Probation) => match self.probation_until.get(gateway) {
                Some(until) if *until > now => PROBATION_SCORE_FACTOR,
                _ => 1.0, // window elapsed → full re-admission
            },
            _ => 1.0,
        }
    }

    /// Drop bookkeeping for gateways no longer known.
    pub fn prune(&mut self, known: &[PeerId]) {
        let known: Vec<PeerId> = known.to_vec();
        self.states.retain(|id, _| known.contains(id));
        self.failure_counters.retain(|id, _| known.contains(id));
        self.re_admission_strikes.retain(|id, _| known.contains(id));
        self.probation_until.retain(|id, _| known.contains(id));
        self.last_success.retain(|id, _| known.contains(id));
    }
}

/// Re-admission cycles that escalate a probation-recidivist to hard-exclusion.
pub const HARD_FAIL_STRIKES: u32 = 2;

/// The gateway discovery + selection engine (GW-001).
#[derive(Debug, Clone)]
pub struct GatewayManager {
    gateways: HashMap<PeerId, GatewayCandidate>,
    health: GatewayHealthMonitor,
    /// Capability of *this node's own* uplink, if any (set from the
    /// INTERNET-001/LoRa transport state).
    self_capability: Option<GatewayCapability>,
}

impl Default for GatewayManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayManager {
    pub fn new() -> Self {
        GatewayManager {
            gateways: HashMap::new(),
            health: GatewayHealthMonitor::new(),
            self_capability: None,
        }
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
        path: Vec<PeerId>,
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
            // Registry full with a genuinely new candidate: evict the oldest
            // confirmed gateway to stay bounded (REDTEAM-02).
            if let Some(oldest) = self
                .gateways
                .values()
                .min_by_key(|g| g.last_confirmed)
                .map(|g| g.node_id)
            {
                self.withdraw(&oldest);
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
                path,
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
            Some(TopologyEvent::GatewayChanged {
                gateway_id: node_id,
                gateway_type,
                available: true,
            })
        } else {
            None
        }
    }

    /// Withdraw a gateway (peer lost, capability removed, or this node's own
    /// uplink dropped). Returns the `GatewayChanged { available: false }` event.
    pub fn withdraw(&mut self, node_id: &PeerId) -> Option<TopologyEvent> {
        if let Some(removed) = self.gateways.remove(node_id) {
            tracing::info!(
                event = event::GW_WITHDRAWN,
                gateway = %node_id.short(),
                gateway_type = %removed.capability.gateway_type.as_str(),
                reason = "stale",
                "gateway candidate withdrawn"
            );
            Some(TopologyEvent::GatewayChanged {
                gateway_id: *node_id,
                gateway_type: removed.capability.gateway_type.as_str().to_string(),
                available: false,
            })
        } else {
            None
        }
    }

    // --- Failure handling ---

    /// Record a delivery ACK timeout. Returns `true` when the gateway was
    /// newly excluded (caller should re-route and surface the change).
    pub fn record_ack_timeout(&mut self, gateway: PeerId) -> bool {
        self.health.record_ack_timeout(gateway)
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
    pub fn select(&self, msg_priority: MessagePriority) -> GatewaySelection {
        self.select_at(msg_priority, Instant::now())
    }

    fn select_at(&self, msg_priority: MessagePriority, now: Instant) -> GatewaySelection {
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

    fn select_inner(&self, msg_priority: MessagePriority, now: Instant) -> GatewaySelection {
        let mut scored: Vec<(f32, PeerId)> = self
            .gateways
            .values()
            .map(|g| {
                let factor = self.health.score_factor(&g.node_id, now);
                (
                    compute_gateway_quality(&g.capability, msg_priority) * factor,
                    g.node_id,
                )
            })
            .filter(|(score, _)| score.is_finite() && *score > MIN_QUALITY_FLOOR)
            .collect();
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
            _ => GatewaySelection::Single {
                gateway: scored[0].1,
                score: scored[0].0,
            },
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
                let _ = self.adopt_from_neighbor(nb.peer_id, cap, 0, Vec::new());
            }
        }
        // Diff-based withdrawal: any registered gateway that is no longer a
        // tagged, linked-up neighbor leaves the registry (REDTEAM-02).
        let known: Vec<PeerId> = self.gateways.keys().copied().collect();
        for node_id in known {
            if !current.contains(&node_id) {
                self.withdraw(&node_id);
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

/// Build the self-uplink capability for an Internet transport that has just
/// become connected. Called by the transport/manager layer on
/// `TransportState::Connected` (INTERNET-001).
pub fn self_internet_gateway(
    transport_id: impl Into<String>,
    cost: crate::transport::TransportCost,
    caps: &crate::transport::TransportCapabilities,
) -> GatewayCapability {
    let mut cap = GatewayCapability::new(GatewayType::Internet, transport_id);
    cap.bandwidth_bps = caps.typical_throughput_bps;
    cap.latency_ms = caps.typical_latency_ms;
    cap.cost_factor = ((cost.monetary_cost_per_kb / 10.0) as f32).clamp(0.0, 1.0);
    cap.current_load = cost.congestion_level.clamp(0.0, 1.0);
    cap.reliability_score = 0.9;
    cap
}

/// Clamp/sanitize float fields at the adoption boundary (REDTEAM-04): NaN or
/// out-of-range values from an untrusted advertisement must not poison the
/// quality score (which would de-rank a victim gateway) or win selection.
fn sanitize_capability(mut cap: GatewayCapability) -> GatewayCapability {
    cap.reliability_score = sanitize_f32(cap.reliability_score, 0.6);
    cap.cost_factor = sanitize_f32(cap.cost_factor, 0.0);
    cap.current_load = sanitize_f32(cap.current_load, 0.0);
    cap.duty_cycle_remaining = sanitize_f32(cap.duty_cycle_remaining, 1.0);
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
    fn health_probation_expires_to_full_score() {
        let mut h = GatewayHealthMonitor::new();
        let g = pid(10);
        for _ in 0..FAILURE_THRESHOLD {
            let _ = h.record_ack_timeout(g);
        }
        h.handle_advertisement(g);
        let t0 = Instant::now();
        assert_eq!(h.score_factor(&g, t0), PROBATION_SCORE_FACTOR);
        let after_window = t0 + RECOVERY_WINDOW + Duration::from_secs(1);
        assert_eq!(h.score_factor(&g, after_window), 1.0);
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
        let _ = m.adopt_from_neighbor(pid(5), cap, 0, vec![]);
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
        let _ = m.adopt_from_neighbor(a, cap_a, 0, vec![]);
        let _ = m.adopt_from_neighbor(b, cap_b, 0, vec![]);
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
                vec![],
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
            vec![],
        );
        let _ = m.adopt_from_neighbor(
            pid(2),
            rich_cap(GatewayType::Lora, 1_000_000, 1000, 0.9, 0.0),
            0,
            vec![],
        );
        let _ = m.adopt_from_neighbor(
            pid(3),
            rich_cap(GatewayType::Internet, 10_000_000, 100, 0.7, 0.1),
            0,
            vec![],
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
        let m = GatewayManager::new();
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
            vec![],
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
            vec![],
        );
        let _ = m.adopt_from_neighbor(
            pid(2),
            rich_cap(GatewayType::Internet, 20_000_000, 80, 0.85, 0.05),
            0,
            vec![],
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
            vec![],
        );
        assert_eq!(m.len(), 1);
        let ev = m.withdraw(&pid(4)).expect("event");
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
        assert!(cap.cost_factor > 0.0);
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
