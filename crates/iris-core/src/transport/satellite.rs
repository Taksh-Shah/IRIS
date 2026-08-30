//! Satellite gateway transport — SAT-001.
//!
//! Implements the last-resort satellite transport as an **adapter-injected**
//! node (the `BleAdapter`/`LoRaLinkAdapter` seam lesson): a platform-neutral
//! Rust core plus a [`SatelliteLinkAdapter`] trait whose implementations talk
//! to **AT-command SBD modems** (RockBLOCK 9603 / 9602-N, Iridium 9604) over
//! serial/USB owned by the adapter crate. Physical terminal integration is
//! hardware-gated (BLK-0005); all simulated evidence is **SIMULATION_VALIDATED
//! only** (DEC-SAT-0007).
//!
//! Carrier budgets (96xx-class SBD): **MO ≤340 B / MT ≤270 B**; handsets are
//! larger but out of scope. Standard CRYPTO-001 envelopes are carried
//! **VERBATIM** into SBD payloads — there is NO on-air IRIS header here and
//! crypto material is never truncated (DEC-SAT-0002). Eligibility is
//! EmergencyOnly(P0–P2) with a transport-side hard gate; P3+ is rejected.
//!
//! The regulatory analog on satellite is MONEY, not spectrum: a SOS-exempt
//! [`SatelliteCostGuard`] enforces hourly-count + daily-INR budgets with P0
//! NEVER blocked (industry pattern: Zoleo/Garmin exempt SOS from quotas) and
//! persisted counters via the [`CostLedger`] seam. RX mailbox checks bill too.
//!
//! Security posture = **HOSTILE PIPE** (DEC-SAT-0005): the Iridium L-band link
//! has proven cloning/spoofable-downlink/no-encryption weaknesses
//! (arXiv:2603.12062) — every inbound envelope MUST pass CRYPTO-001 verify
//! ABOVE the transport before any effect. Ring Alerts are wake-up hints only;
//! inbound attribution is the zero-PeerId fallback (WAW-RT-007 precedent).
//! Satellite is best-effort redundancy, never guaranteed delivery.

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::Stream;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use tokio::sync::broadcast;

use super::lora::BacklogQueue;
use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::{
    AtomicState, BatteryCostModel, EwmaGoodput, Transport, TransportCapabilities, TransportCost,
    TransportCostClass, TransportId, TransportState, TransportStateEvent,
};
use crate::TransportError;

/// SBD Mobile-Originated budget (96xx modules): hard cap for `send()`.
pub const MAX_SBD_MO_BYTES: usize = 340;
/// SBD Mobile-Terminated budget (96xx modules): hard cap for inbound frames.
pub const MAX_SBD_MT_BYTES: usize = 270;
/// AC-9: asynchronous custody-style retry base — never assume RTT <90 s.
pub const RETRY_BASE_MS: u64 = 300_000;
/// AC-9: ±20% jitter applied to the retry base.
pub const RETRY_JITTER_FRACTION: f32 = 0.20;
/// Cost-guard rolling window (1 h message cap).
pub const HOURLY_WINDOW_MS: u64 = 3_600_000;
/// Cost-guard daily budget window.
pub const DAILY_WINDOW_MS: u64 = 86_400_000;
/// Provisional FX for folding USD-priced mailbox checks into the INR budget
/// (G-5: provisional until hardware benchmark).
pub const SAT_USD_INR: f64 = 90.0;
/// Provisional per-message TX cost estimate folded into the daily INR budget.
pub const SAT_EST_TX_COST_INR: f64 = 10.0;

/// Battery draw model for an Iridium modem (TRANSPORT_ABSTRACTION.md shape).
/// DESIGN ESTIMATE only — hardware bench pending (BLK-0005).
pub const SAT_COST: BatteryCostModel = BatteryCostModel {
    scan_ma: 35.0,
    advertise_ma: 0.0,
    connected_idle_ma: 35.0,
    tx_ma_per_kbps: 2.0,
    rx_ma_per_kbps: 1.0,
};

/// Satellite service provider. v1 ships IridiumSbd only; further backends are
/// DOCUMENTED-not-built behind the same adapter seam (`#[non_exhaustive]` so
/// downstream matches must carry a wildcard arm).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SatelliteProvider {
    /// Legacy Short Burst Data over AT-command 96xx-class modems.
    IridiumSbd,
}

impl SatelliteProvider {
    /// Raw-rate hint used by cost snapshots (SBD ≈2.4 kbps session air rate;
    /// sustained effective rate is quota-bound far below this).
    pub fn raw_rate_bps(self) -> u64 {
        match self {
            SatelliteProvider::IridiumSbd => 2_400,
        }
    }
}

/// Rule set consumed by the [`SatelliteCostGuard`] (the money analog of
/// LoRa's `ComplianceConfig` — spend is the legal/commercial floor here).
#[derive(Debug, Clone, PartialEq)]
pub struct CostGuardConfig {
    /// Non-emergency transmissions allowed per rolling hour (P0 exempt).
    pub hourly_msg_cap: u32,
    /// Daily monetary budget for non-emergency traffic + RX checks (INR).
    pub daily_budget_inr: f64,
    /// Provisional per-message TX cost folded into the daily budget.
    pub est_cost_per_msg_inr: f64,
    /// Per-mailbox-check RX cost (≈$0.05 × provisional FX), folded in.
    pub mailbox_check_cost_inr: f64,
    /// RF-22: minimum interval between billed mailbox polls (ms). Calls that
    /// arrive sooner are skipped without billing or I/O. Default 300 s —
    /// derived from the Iridium check cost: 45 checks × ₹4.5 ≈ daily budget.
    pub min_mailbox_interval_ms: u64,
    /// When true, discretionary P1/P2 sends require confirmation-hook approval.
    pub confirm_discretionary_p1_p2: bool,
}

impl Default for CostGuardConfig {
    fn default() -> Self {
        CostGuardConfig {
            hourly_msg_cap: 10,
            daily_budget_inr: 200.0,
            est_cost_per_msg_inr: SAT_EST_TX_COST_INR,
            mailbox_check_cost_inr: 0.05 * SAT_USD_INR,
            min_mailbox_interval_ms: RETRY_BASE_MS, // 300 s — matches AC-9 custody base
            confirm_discretionary_p1_p2: true,
        }
    }
}

// ==== SBD carrier framing (D-2: envelopes VERBATIM — zero added bytes) ====

/// Direction of an SBD transfer — budgets are asymmetric.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SbdDirection {
    MobileOriginated,
    MobileTerminated,
}

/// Malformed-input error — defensive parse never panics (BLE-002 AC-6 pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SbdError {
    TooLargeMo(usize),
    TooLargeMt(usize),
    Empty,
}

impl std::fmt::Display for SbdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SbdError::TooLargeMo(n) => {
                write!(f, "MO payload {n} B exceeds {MAX_SBD_MO_BYTES}-B budget")
            }
            SbdError::TooLargeMt(n) => {
                write!(f, "MT frame {n} B exceeds {MAX_SBD_MT_BYTES}-B budget")
            }

            SbdError::Empty => write!(f, "empty frame"),
        }
    }
}

impl std::error::Error for SbdError {}

/// Typed accessor over ONE SBD payload carrying a standard IRIS envelope
/// **verbatim** (DEC-SAT-0002: zero added bytes, zero truncation).
///
/// Because the modem delivers exact-length payloads and envelopes are opaque
/// to this transport, there is NO on-air IRIS header here — the frame type
/// exists solely to enforce the directional caps
/// ([`MAX_SBD_MO_BYTES`]/[`MAX_SBD_MT_BYTES`]) and give the receive path its
/// defensive-parse seam (AC-7/AC-8). Field-level validation happens ABOVE,
/// at the envelope layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SbdFrame {
    pub direction: SbdDirection,
    pub envelope: Vec<u8>,
}

impl SbdFrame {
    /// Wrap an OUTBOUND envelope (MO budget ≤340 B total, verbatim).
    pub fn new_mo(envelope: &[u8]) -> Result<SbdFrame, SbdError> {
        if envelope.is_empty() {
            return Err(SbdError::Empty);
        }
        if envelope.len() > MAX_SBD_MO_BYTES {
            return Err(SbdError::TooLargeMo(envelope.len()));
        }
        Ok(SbdFrame {
            direction: SbdDirection::MobileOriginated,
            envelope: envelope.to_vec(),
        })
    }

    /// Wrap an INBOUND payload (MT budget ≤270 B total, verbatim). Caps +
    /// emptiness are all this layer can know; content trust lives above.
    pub fn new_mt(bytes: &[u8]) -> Result<SbdFrame, SbdError> {
        if bytes.is_empty() {
            return Err(SbdError::Empty);
        }
        if bytes.len() > MAX_SBD_MT_BYTES {
            return Err(SbdError::TooLargeMt(bytes.len()));
        }
        Ok(SbdFrame {
            direction: SbdDirection::MobileTerminated,
            envelope: bytes.to_vec(),
        })
    }

    /// The exact bytes placed on (or read from) the SBD wire.
    pub fn as_bytes(&self) -> &[u8] {
        &self.envelope
    }
}
// ==== Cost ledger — mutation-time persistence seam (AC-3) ====

/// Every observable guard mutation (persistence sinks implement this).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedgerEvent {
    TxAdmitted { emergency: bool },
    TxRefusedHourly,
    TxRefusedDaily,
    TxRefunded,
    MailboxCheckBilled,
}

/// Persistence seam: called on EVERY guard mutation. The in-memory impl backs
/// tests; a durable sink survives reboots (disaster-context requirement).
pub trait CostLedger: Send + Sync {
    fn record(&self, event: LedgerEvent);
}

/// Default ledger keeping an in-memory event log (also the test oracle).
/// SAT-RT-109: BOUNDED at [`IN_MEMORY_LEDGER_CAP`] events (drop-oldest) so a
/// long-running gateway polling mailbox checks cannot grow memory forever;
/// durable sinks are the production persistence path.
#[derive(Debug)]
pub struct InMemoryLedger {
    events: Mutex<VecDeque<LedgerEvent>>,
}

/// SAT-RT-109: retention bound for the in-memory oracle ledger.
pub const IN_MEMORY_LEDGER_CAP: usize = 4096;

impl InMemoryLedger {
    pub fn new() -> Self {
        InMemoryLedger {
            events: Mutex::new(VecDeque::new()),
        }
    }

    /// Oldest-to-newest snapshot of retained events (bounded).
    pub fn events(&self) -> Vec<LedgerEvent> {
        self.events.lock().unwrap_or_else(|p| p.into_inner()).iter().copied().collect()
    }
}

impl Default for InMemoryLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl CostLedger for InMemoryLedger {
    fn record(&self, event: LedgerEvent) {
        let mut q = self.events.lock().unwrap_or_else(|p| p.into_inner());
        q.push_back(event);
        while q.len() > IN_MEMORY_LEDGER_CAP {
            q.pop_front();
        }
    }
}

/// Multi-sink ledger proving the persistence seam supports several writers at
/// once (e.g. in-memory oracle + durable store) — the double-write harness.
pub struct FanoutLedger {
    sinks: Vec<Arc<dyn CostLedger>>,
}

impl FanoutLedger {
    pub fn new(sinks: Vec<Arc<dyn CostLedger>>) -> Self {
        FanoutLedger { sinks }
    }
}

impl CostLedger for FanoutLedger {
    fn record(&self, event: LedgerEvent) {
        for sink in &self.sinks {
            sink.record(event);
        }
    }
}

// ==== Cost guard (D-3 / DEC-SAT-0003) ====

/// RF-18: monotonic guard — mirrors lora.rs `MonotonicGuard` exactly. The
/// spend guard uses a CAS-based backward clamp (SAT-RT-102) but had no
/// forward-jump defence; `MonotonicGuard` bounds how far ahead `now()` can
/// jump per call to at most "previous output + real elapsed + tolerance".
/// Used when a real wall clock is injected (`new()`) but skipped when a
/// test-controlled clock is injected (`with_clock()`).
struct SatMonotonicGuard {
    anchor: std::time::Instant,
    last_out: u64,
    last_elapsed_ms: u64,
}

impl SatMonotonicGuard {
    fn new() -> Self {
        SatMonotonicGuard {
            anchor: std::time::Instant::now(),
            last_out: 0,
            last_elapsed_ms: 0,
        }
    }

    fn bound(&mut self, raw: u64) -> u64 {
        let elapsed = self.anchor.elapsed().as_millis() as u64;
        if self.last_out == 0 {
            self.last_out = raw;
            self.last_elapsed_ms = elapsed;
            return raw;
        }
        let real_delta = elapsed.saturating_sub(self.last_elapsed_ms);
        let ceiling = self
            .last_out
            .saturating_add(real_delta)
            .saturating_add(SAT_MONOTONIC_TOLERANCE_MS);
        let out = raw.min(ceiling).max(self.last_out);
        self.last_out = out;
        self.last_elapsed_ms = elapsed;
        out
    }
}

const SAT_MONOTONIC_TOLERANCE_MS: u64 = 1_000;

/// Successful admission. `requires_confirmation` is true ONLY for
/// discretionary P1/P2 when the config asks for confirmation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardAdmission {
    pub requires_confirmation: bool,
    /// SAT-RT-103: exact-token refund handle (0 = emergency/untracked).
    pub token: u64,
}

/// Why (and when) a non-P0 send was refused. Never a panic; the routing
/// layer schedules around it or queues (queue-not-drop, AC-4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardRefusalReason {
    HourlyCap,
    DailyBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuardRefusal {
    pub reason: GuardRefusalReason,
    pub retry_at_unix_ms: u64,
}

impl std::fmt::Display for GuardRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?} budget exhausted; retry after {} ms",
            self.reason, self.retry_at_unix_ms
        )
    }
}

type NowFn = Box<dyn Fn() -> u64 + Send + Sync>;
type ConfirmHook = Option<Arc<dyn Fn(MessagePriority) -> bool + Send + Sync>>;

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// SOS-exempt spend guard (DEC-SAT-0003): **P0 is never blocked**; P1/P2 face
/// an hourly message cap plus a daily INR budget (TX estimates + billed RX
/// mailbox checks), each with queue-not-drop recovery semantics. Counters are
/// recorded through the [`CostLedger`] seam on every mutation.
pub struct SatelliteCostGuard {
    cfg: CostGuardConfig,
    /// Non-emergency admissions this hour, oldest first (SAT-RT-103: each
    /// entry carries its admission token for exact-token refunds).
    hourly: Mutex<VecDeque<HourEntry>>,
    day_index: AtomicU64,
    /// RF-21: P0 sends metered separately — exempt from REFUSAL but still
    /// counted so daily_spend_inr() reports the true financial picture.
    p0_today: AtomicU32,
    p12_today: AtomicU32,
    mailbox_today: AtomicU32,
    ledger: Arc<dyn CostLedger>,
    now_fn: NowFn,
    /// SAT-RT-102: monotonic floor — accepted time never decreases, so a
    /// backward wall-clock step can neither reset daily counters nor freeze
    /// hourly pruning (lesson mirrored verbatim from lora RT-102).
    last_ms: AtomicU64,
    /// RF-18: forward-jump guard. `None` when a test-controlled clock is
    /// injected (via `with_clock`); `Some` for real wall clock (`new`).
    monotonic: Option<Mutex<SatMonotonicGuard>>,
    /// SAT-RT-103: unique admission tokens for exact-token refunds.
    next_token: AtomicU64,
    /// RF-22: monotonic timestamp of the last billed mailbox check. Used to
    /// enforce `cfg.min_mailbox_interval_ms` and prevent budget exhaustion
    /// from high-frequency polling.
    last_mailbox_ms: AtomicU64,
}

/// One non-emergency admission inside the rolling hourly window.
#[derive(Debug, Clone, Copy)]
struct HourEntry {
    ts: u64,
    token: u64,
}

const DAY_MS: u64 = DAILY_WINDOW_MS;

impl SatelliteCostGuard {
    pub fn new(cfg: CostGuardConfig, ledger: Arc<dyn CostLedger>) -> Self {
        SatelliteCostGuard {
            cfg,
            hourly: Mutex::new(VecDeque::new()),
            day_index: AtomicU64::new(u64::MAX),
            p0_today: AtomicU32::new(0),
            p12_today: AtomicU32::new(0),
            mailbox_today: AtomicU32::new(0),
            ledger,
            now_fn: Box::new(system_now_ms),
            last_ms: AtomicU64::new(0),
            // RF-18: real clock path — guard against forward wall-clock jumps.
            monotonic: Some(Mutex::new(SatMonotonicGuard::new())),
            next_token: AtomicU64::new(1),
            last_mailbox_ms: AtomicU64::new(0),
        }
    }

    pub fn with_clock(cfg: CostGuardConfig, ledger: Arc<dyn CostLedger>, now_fn: NowFn) -> Self {
        SatelliteCostGuard {
            cfg,
            hourly: Mutex::new(VecDeque::new()),
            day_index: AtomicU64::new(u64::MAX),
            p0_today: AtomicU32::new(0),
            p12_today: AtomicU32::new(0),
            mailbox_today: AtomicU32::new(0),
            ledger,
            now_fn,
            last_ms: AtomicU64::new(0),
            // No forward-jump guard with an injected clock — tests must be
            // able to jump time freely.
            monotonic: None,
            next_token: AtomicU64::new(1),
            last_mailbox_ms: AtomicU64::new(0),
        }
    }

    /// SAT-RT-102 + RF-18: monotonic-clamped clock — backward steps are
    /// ignored (RT-102); forward jumps are bounded to real elapsed time when
    /// the guard is present (RF-18). Day rollover resets today's counters
    /// exactly once per boundary on the clamped timeline.
    fn now(&self) -> u64 {
        let raw = (self.now_fn)();
        // RF-18: clamp forward jumps to at most "prev + real elapsed + tolerance".
        let t = match &self.monotonic {
            Some(g) => g.lock().unwrap_or_else(|p| p.into_inner()).bound(raw),
            None => raw,
        };
        let mut prev_ms = self.last_ms.load(Ordering::Acquire);
        while t > prev_ms {
            match self
                .last_ms
                .compare_exchange(prev_ms, t, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(actual) => prev_ms = actual,
            }
        }
        let accepted = self.last_ms.load(Ordering::Acquire);
        let d = accepted / DAY_MS;
        let prev = self.day_index.load(Ordering::Acquire);
        if d != prev {
            // Day rollover: reset today's counters exactly once.
            if self
                .day_index
                .compare_exchange(prev, d, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                self.p0_today.store(0, Ordering::Release);
                self.p12_today.store(0, Ordering::Release);
                self.mailbox_today.store(0, Ordering::Release);
            }
        }
        accepted
    }

    fn prune_hourly_locked(q: &mut VecDeque<HourEntry>, now: u64) {
        while let Some(front) = q.front() {
            if now.saturating_sub(front.ts) >= HOURLY_WINDOW_MS {
                q.pop_front();
            } else {
                break;
            }
        }
    }

    fn daily_spend_cents(&self) -> f64 {
        // RF-21: include P0 in reported spend (exempt from REFUSAL, not from
        // accounting — "metered but never blocked").
        let p0 = self.p0_today.load(Ordering::Relaxed) as f64;
        let p12 = self.p12_today.load(Ordering::Relaxed) as f64;
        let mb = self.mailbox_today.load(Ordering::Relaxed) as f64;
        (p0 + p12) * self.cfg.est_cost_per_msg_inr * 100.0
            + mb * self.cfg.mailbox_check_cost_inr * 100.0
    }

    /// Reserve a transmission slot. **P0 always admits** — emergencies never
    /// wait for money (DEC-SAT-0003). P1/P2 face hourly cap + daily budget.
    pub fn admit_tx(&self, priority: MessagePriority) -> Result<GuardAdmission, GuardRefusal> {
        let t = self.now();
        let emergency = matches!(priority, MessagePriority::P0 | MessagePriority::P1);
        if matches!(priority, MessagePriority::P0) {
            // RF-21: meter P0 — exempt from REFUSAL but still counted so
            // daily_spend_inr() reports the true financial picture. Reset
            // on day boundary like p12_today (handled in now()).
            self.p0_today.fetch_add(1, Ordering::Relaxed);
            self.ledger.record(LedgerEvent::TxAdmitted { emergency });
            // Token 0 marks it unrefundable (SAT-RT-101: classification
            // authority lives ABOVE the transport — RECORDED obligation).
            return Ok(GuardAdmission {
                requires_confirmation: false,
                token: 0,
            });
        }
        let mut q = self.hourly.lock().unwrap_or_else(|p| p.into_inner());
        Self::prune_hourly_locked(&mut q, t);
        if q.len() as u32 >= self.cfg.hourly_msg_cap {
            self.ledger.record(LedgerEvent::TxRefusedHourly);
            let retry_at_unix_ms = match q.front() {
                Some(oldest) => oldest.ts + HOURLY_WINDOW_MS + 1 - t,
                None => HOURLY_WINDOW_MS + 1,
            };
            return Err(GuardRefusal {
                reason: GuardRefusalReason::HourlyCap,
                retry_at_unix_ms: t.saturating_add(retry_at_unix_ms),
            });
        }
        let spend = self.daily_spend_cents();
        let next = spend + self.cfg.est_cost_per_msg_inr * 100.0;
        let budget = self.cfg.daily_budget_inr * 100.0;
        if next > budget {
            self.ledger.record(LedgerEvent::TxRefusedDaily);
            let d = t / DAY_MS;
            return Err(GuardRefusal {
                reason: GuardRefusalReason::DailyBudget,
                retry_at_unix_ms: (d + 1) * DAY_MS + 1,
            });
        }
        let token = self.next_token.fetch_add(1, Ordering::Relaxed);
        q.push_back(HourEntry { ts: t, token });
        self.p12_today.fetch_add(1, Ordering::Relaxed);
        self.ledger.record(LedgerEvent::TxAdmitted { emergency });
        Ok(GuardAdmission {
            requires_confirmation: self.cfg.confirm_discretionary_p1_p2,
            token,
        })
    }

    /// SAT-RT-103: refund by EXACT admission token — removes that specific
    /// reservation if it is still inside the hourly window, decrements
    /// today's counter only when the admission happened today, and records
    /// the ledger event only when something was actually refunded. Late or
    /// duplicate refunds are safe no-ops; refunds only ever REMOVE spend.
    /// Returns true when a live reservation was refunded. (RT-104 lesson:
    /// TX failures after admission must not burn the budget.)
    pub fn refund_tx(&self, token: u64) -> bool {
        if token == 0 {
            return false; // emergency admissions are untracked by design
        }
        let t = self.now();
        let removed_day;
        {
            let mut q = self.hourly.lock().unwrap_or_else(|p| p.into_inner());
            Self::prune_hourly_locked(&mut q, t);
            let pos = q.iter().position(|e| e.token == token);
            let Some(pos) = pos else {
                return false; // already pruned/expired/unknown — nothing to refund
            };
            let entry = q.remove(pos).expect("index from live iterator");
            removed_day = entry.ts / DAY_MS;
        }
        if removed_day == self.day_index.load(Ordering::Acquire) {
            // Only unwind TODAY'S counter; yesterday's budget already closed.
            self.p12_today
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                    if v > 0 {
                        Some(v - 1)
                    } else {
                        None
                    }
                })
                .ok();
        }
        self.ledger.record(LedgerEvent::TxRefunded);
        true
    }

    /// Bill one RX mailbox poll (checks cost money even when nothing arrives
    /// — FC-7). RF-22: returns `false` (skip this poll) when the last billed
    /// check is less than `cfg.min_mailbox_interval_ms` ago, preventing
    /// high-frequency callers from exhausting the daily budget on empty polls.
    pub fn record_mailbox_check(&self) -> bool {
        let t = self.now();
        let last = self.last_mailbox_ms.load(Ordering::Acquire);
        if t.saturating_sub(last) < self.cfg.min_mailbox_interval_ms {
            return false;
        }
        // CAS: only one thread bills per interval window.
        if self
            .last_mailbox_ms
            .compare_exchange(last, t, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return false;
        }
        self.mailbox_today.fetch_add(1, Ordering::Relaxed);
        self.ledger.record(LedgerEvent::MailboxCheckBilled);
        true
    }

    /// Remaining slots this hour for non-emergency traffic.
    pub fn hourly_remaining(&self) -> u32 {
        let mut q = self.hourly.lock().unwrap_or_else(|p| p.into_inner());
        Self::prune_hourly_locked(&mut q, self.now());
        self.cfg.hourly_msg_cap.saturating_sub(q.len() as u32)
    }

    /// Today's estimated spend (INR) across TX + billed mailbox checks.
    pub fn daily_spend_inr(&self) -> f64 {
        self.daily_spend_cents() / 100.0
    }
}

// ==== Link adapter seam — AT-modem vs sim (D-1) ====

/// Module-side status surfaced via the link (SIMULATION_VALIDATED when from
/// the simulated adapter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SatelliteStatus {
    pub signal_quality_pct: u8,
    pub battery_pct: u8,
}

/// Link-layer failure. `HardwareGated` marks the BLK-0005 boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatelliteLinkError {
    HardwareGated,
    Closed,
    Io(String),
}

impl std::fmt::Display for SatelliteLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SatelliteLinkError::HardwareGated => write!(f, "link hardware-gated (BLK-0005)"),
            SatelliteLinkError::Closed => write!(f, "link closed"),
            SatelliteLinkError::Io(m) => write!(f, "link io: {m}"),
        }
    }
}

impl std::error::Error for SatelliteLinkError {}

/// MG-39/MG-41: mirrors `lora.rs`'s `lora_link_err` — every call site used
/// to wrap *any* `SatelliteLinkError` variant into `TransportError::Io(String)`
/// uniformly, collapsing "hardware gated" (permanent), "link closed" (needs
/// a fresh connect), and a genuine I/O failure (may be transient) into one
/// opaque string.
fn satellite_link_err(context: &str, e: SatelliteLinkError) -> TransportError {
    match e {
        SatelliteLinkError::HardwareGated => TransportError::HardwareUnavailable,
        SatelliteLinkError::Closed => TransportError::NotConnected,
        SatelliteLinkError::Io(msg) => TransportError::Io {
            kind: std::io::ErrorKind::Other,
            msg: format!("{context}: {msg}"),
        },
    }
}

/// The seam between the IRIS core and an external satellite modem/hotspot
/// (`open/close/tx/rx/status`) — abstracts AT-serial SBD modems so the Rust
/// core never depends on a module-family API (BleAdapter/LoRaLinkAdapter
/// lesson, DEC-SAT-0001).
///
/// # Implementation contract (BLK-0005 firmware notes)
/// - After a successful `close()`, `tx()`/`rx()` MUST return
///   [`SatelliteLinkError::Closed`].
/// - `tx()` carries ONE MO payload of at most [`MAX_SBD_MO_BYTES`] bytes (an IRIS
///   envelope verbatim — implementations MUST NOT add framing or truncate).
/// - `rx()` returns ONE MT payload of at most [`MAX_SBD_MT_BYTES`] bytes or `None`;
///   batched reads are buffered inside the implementation.
#[async_trait]
pub trait SatelliteLinkAdapter: Send + Sync + 'static {
    async fn open(&self) -> Result<(), SatelliteLinkError>;
    async fn close(&self) -> Result<(), SatelliteLinkError>;
    async fn tx(&self, payload: &[u8]) -> Result<(), SatelliteLinkError>;
    async fn rx(&self) -> Result<Option<Vec<u8>>, SatelliteLinkError>;
    async fn status(&self) -> Result<SatelliteStatus, SatelliteLinkError>;
}

/// AT-command SBD modem surface (RockBLOCK 9603/9602-N, Iridium 9604) —
/// **shape only**: every operation reports
/// [`SatelliteLinkError::HardwareGated`] until the BLK-0005 hardware leg lands.
#[derive(Debug, Clone)]
pub struct AtSbdModemAdapter {
    pub port: String,
    pub baud: u32,
}

impl AtSbdModemAdapter {
    pub fn new(port: impl Into<String>, baud: u32) -> Self {
        AtSbdModemAdapter {
            port: port.into(),
            baud,
        }
    }
}

#[async_trait]
impl SatelliteLinkAdapter for AtSbdModemAdapter {
    async fn open(&self) -> Result<(), SatelliteLinkError> {
        Err(SatelliteLinkError::HardwareGated)
    }
    async fn close(&self) -> Result<(), SatelliteLinkError> {
        Err(SatelliteLinkError::HardwareGated)
    }
    async fn tx(&self, _payload: &[u8]) -> Result<(), SatelliteLinkError> {
        Err(SatelliteLinkError::HardwareGated)
    }
    async fn rx(&self) -> Result<Option<Vec<u8>>, SatelliteLinkError> {
        Err(SatelliteLinkError::HardwareGated)
    }
    async fn status(&self) -> Result<SatelliteStatus, SatelliteLinkError> {
        Err(SatelliteLinkError::HardwareGated)
    }
}

/// Deterministic in-memory link for tests/conformance. Two instances joined
/// by [`SimulatedSatelliteAdapter::connect_pair`] exchange payloads with
/// configurable propagation delay + outage probability; randomness is seeded
/// ChaCha8 (SIM-001 precedent).
#[derive(Clone)]
pub struct SimulatedSatelliteAdapter(Arc<SimWire>);

struct SimWire {
    opened: AtomicBool,
    outage_probability: f32,
    latency_base_ms: u64,
    latency_spread_ms: u64,
    state: Mutex<SimWireState>,
}

struct SimWireState {
    peer: Option<Arc<SimWire>>,
    inbound: VecDeque<PendingPayload>,
    rng: ChaCha8Rng,
}

struct PendingPayload {
    ready_at: Instant,
    data: Vec<u8>,
}

/// Simulation knobs for [`SimulatedSatelliteAdapter`] (defaults keep tests
/// fast; realistic deployments use 20–90 s end-to-end latency).
#[derive(Debug, Clone)]
pub struct SimSatConfig {
    pub latency_base_ms: u64,
    pub latency_spread_ms: u64,
    pub outage_probability: f32,
    pub seed: u64,
}

impl Default for SimSatConfig {
    fn default() -> Self {
        SimSatConfig {
            latency_base_ms: 0,
            latency_spread_ms: 0,
            outage_probability: 0.0,
            seed: 42,
        }
    }
}

impl SimulatedSatelliteAdapter {
    pub fn new(cfg: SimSatConfig) -> Self {
        SimulatedSatelliteAdapter(Arc::new(SimWire {
            opened: AtomicBool::new(false),
            outage_probability: cfg.outage_probability.clamp(0.0, 1.0),
            latency_base_ms: cfg.latency_base_ms,
            latency_spread_ms: cfg.latency_spread_ms,
            state: Mutex::new(SimWireState {
                peer: None,
                inbound: VecDeque::new(),
                rng: ChaCha8Rng::seed_from_u64(cfg.seed),
            }),
        }))
    }

    /// Wire two adapters to each other (both directions).
    pub fn connect_pair(a: &SimulatedSatelliteAdapter, b: &SimulatedSatelliteAdapter) {
        a.0.state.lock().unwrap_or_else(|p| p.into_inner()).peer = Some(Arc::clone(&b.0));
        b.0.state.lock().unwrap_or_else(|p| p.into_inner()).peer = Some(Arc::clone(&a.0));
    }
}

#[async_trait]
impl SatelliteLinkAdapter for SimulatedSatelliteAdapter {
    async fn open(&self) -> Result<(), SatelliteLinkError> {
        self.0.opened.store(true, Ordering::Release);
        Ok(())
    }

    async fn close(&self) -> Result<(), SatelliteLinkError> {
        self.0.opened.store(false, Ordering::Release);
        Ok(())
    }

    async fn tx(&self, payload: &[u8]) -> Result<(), SatelliteLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(SatelliteLinkError::Closed);
        }
        let peer = {
            let mut st = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
            let lost = rand::Rng::gen_range(&mut st.rng, 0.0..1.0) < self.0.outage_probability;
            if lost {
                return Ok(()); // transmitted during an outage — silently gone
            }
            st.peer.clone()
        };
        if let Some(peer) = peer {
            let spread = self.0.latency_spread_ms;
            let jitter = if spread == 0 {
                0
            } else {
                rand::Rng::gen_range(&mut peer.state.lock().unwrap_or_else(|p| p.into_inner()).rng, 0..=spread)
            };
            let pending = PendingPayload {
                ready_at: Instant::now() + Duration::from_millis(self.0.latency_base_ms + jitter),
                data: payload.to_vec(),
            };
            peer.state.lock().unwrap_or_else(|p| p.into_inner()).inbound.push_back(pending);
        }
        Ok(())
    }

    async fn rx(&self) -> Result<Option<Vec<u8>>, SatelliteLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(SatelliteLinkError::Closed);
        }
        let mut st = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let idx = st.inbound.iter().position(|p| p.ready_at <= now);
        if let Some(idx) = idx {
            let frame = st.inbound.remove(idx).expect("index from live iterator");
            Ok(Some(frame.data))
        } else {
            Ok(None)
        }
    }

    async fn status(&self) -> Result<SatelliteStatus, SatelliteLinkError> {
        // Fixed sim telemetry (SIMULATION_VALIDATED only).
        Ok(SatelliteStatus {
            signal_quality_pct: 80,
            battery_pct: 100,
        })
    }
}

// ==== OBS-001 metrics seam (AC-10) ====

/// Counters surfaced via `iris.transport.satellite.*`.
#[derive(Debug, Default)]
pub struct SatelliteMetrics {
    packets_tx: AtomicU64,
    packets_rx: AtomicU64,
    bytes_tx: AtomicU64,
    bytes_rx: AtomicU64,
    hourly_refusals: AtomicU64,
    daily_refusals: AtomicU64,
    confirmation_denied: AtomicU64,
    tx_failures: AtomicU64,
    malformed_rx: AtomicU64,
    mailbox_checks: AtomicU64,
    /// RF-17: adapter.close() calls that returned an error on teardown.
    close_failures: AtomicU64,
    /// RF-12: backlog entries dropped because the error is permanent
    /// (Protocol / PolicyDenied / MessageTooLarge — will never succeed).
    backlog_protocol_drops: AtomicU64,
}

/// Point-in-time copy of [`SatelliteMetrics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SatelliteMetricsSnapshot {
    pub packets_tx: u64,
    pub packets_rx: u64,
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    pub hourly_refusals: u64,
    pub daily_refusals: u64,
    pub confirmation_denied: u64,
    pub tx_failures: u64,
    pub malformed_rx: u64,
    pub mailbox_checks: u64,
    /// RF-17: adapter close errors on detach or shutdown.
    pub close_failures: u64,
    /// RF-12: backlog entries permanently dropped (Protocol/PolicyDenied/MessageTooLarge).
    pub backlog_protocol_drops: u64,
}

macro_rules! sat_metric_accessors {
    ($($field:ident),+) => {
        impl SatelliteMetrics {
            $(
                fn $field(&self) -> &AtomicU64 { &self.$field }
            )+
            pub fn snapshot(&self) -> SatelliteMetricsSnapshot {
                SatelliteMetricsSnapshot {
                    $( $field: self.$field.load(Ordering::Relaxed), )+
                }
            }
        }
    };
}

sat_metric_accessors!(
    packets_tx,
    packets_rx,
    bytes_tx,
    bytes_rx,
    hourly_refusals,
    daily_refusals,
    confirmation_denied,
    tx_failures,
    malformed_rx,
    mailbox_checks,
    close_failures,
    backlog_protocol_drops
);

// ==== SatelliteTransport — the Transport impl wiring it together ====

fn satellite_capabilities() -> TransportCapabilities {
    TransportCapabilities {
        // SAT-RT-105: advertise the MT budget — sat-to-sat relay rides the
        // 270-B MT leg; MO 340 is gateway-egress-only (documented dead zone).
        max_message_size: MAX_SBD_MT_BYTES,
        supports_broadcast: false,
        supports_unicast: true,
        supports_multicast: false,
        range_m_min: 100_000,
        range_m_max: 20_000_000,
        range_m_typical: 2_000_000,
        typical_throughput_bps: SatelliteProvider::IridiumSbd.raw_rate_bps(),
        typical_latency_ms: 90_000,
        requires_infrastructure: true,
        supports_background_android: false,
        supports_background_ios: false,
        requires_special_hardware: true,
        cost_class: TransportCostClass::Expensive {
            cost_per_message_inr: SAT_EST_TX_COST_INR,
            minimum_cost_inr: SAT_EST_TX_COST_INR,
        },
        regulatory_band: Some(String::from("GMPCS/MSS operator authorization required")),
        conflict_group: crate::transport::RadioConflictGroup::None,
    }
}

/// Satellite gateway transport: wires the SOS-exempt [`SatelliteCostGuard`],
/// the [`SatelliteLinkAdapter`] dongle seam, and OBS-001 counters onto the
/// core `Transport` trait. EmergencyOnly(P0–P2) with a transport-side hard
/// gate; hostile-pipe posture — inbound frames are untrusted until verified
/// ABOVE this transport.
pub struct SatelliteTransport {
    id: TransportId,
    display: String,
    caps: TransportCapabilities,
    state: AtomicState,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    guard: Arc<SatelliteCostGuard>,
    adapter: tokio::sync::RwLock<Option<Arc<dyn SatelliteLinkAdapter>>>,
    backlog: BacklogQueue<SerializedMessage>,
    metrics: SatelliteMetrics,
    last_priority: AtomicU8,
    shutdown_flag: AtomicBool,
    confirm_hook: tokio::sync::RwLock<ConfirmHook>,
    /// MG-17: EWMA goodput tracker.
    ewma: EwmaGoodput,
}

// SYS-2: deadline constants for satellite adapter calls.
// Satellite link establishment (Iridium SBD ISU AT commands) can take up to 60s.
const OPEN_TIMEOUT: Duration = Duration::from_secs(60);
const TX_TIMEOUT: Duration = Duration::from_secs(90);

impl SatelliteTransport {
    pub fn new(id: &str) -> Self {
        let ledger: Arc<dyn CostLedger> = Arc::new(InMemoryLedger::new());
        Self::with_ledger(id, ledger)
    }

    pub fn with_ledger(id: &str, ledger: Arc<dyn CostLedger>) -> Self {
        Self::with_guard(
            id,
            Arc::new(SatelliteCostGuard::new(CostGuardConfig::default(), ledger)),
        )
    }

    pub fn with_guard(id: &str, guard: Arc<SatelliteCostGuard>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        SatelliteTransport {
            id: TransportId::from(id),
            display: String::from("Satellite Gateway"),
            caps: satellite_capabilities(),
            state: AtomicState::default(),
            state_tx,
            incoming_tx,
            guard,
            adapter: tokio::sync::RwLock::new(None),
            backlog: BacklogQueue::new(),
            metrics: SatelliteMetrics::default(),
            last_priority: AtomicU8::new(MessagePriority::P4.as_u8()),
            shutdown_flag: AtomicBool::new(false),
            confirm_hook: tokio::sync::RwLock::new(None),
            ewma: EwmaGoodput::new(),
        }
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        self.state_tx.send(TransportStateEvent {
            transport_id: self.id.clone(),
            new_state: state,
        });
    }

    /// Terminal plug: attach the link adapter, bring it up, promote to
    /// Available. RT-103: an `open()` failure is NOT ignored — a dead device
    /// must not be stored behind Available state.
    pub async fn attach_adapter(
        &self,
        adapter: Arc<dyn SatelliteLinkAdapter>,
    ) -> Result<(), SatelliteLinkError> {
        if self.shutdown_flag.load(Ordering::Acquire) {
            return Err(SatelliteLinkError::Closed);
        }
        tokio::time::timeout(OPEN_TIMEOUT, adapter.open())
            .await
            .map_err(|_| SatelliteLinkError::Io("open timed out".to_string()))??;
        // SAT-RT-108: re-check shutdown INSIDE the slot critical section (a
        // racing shutdown must not be resurrected behind Available state) and
        // close+drop any replaced adapter (double-open port-leak fix).
        {
            let mut slot = self.adapter.write().await;
            if self.shutdown_flag.load(Ordering::Acquire) {
                adapter.close().await.ok();
                return Err(SatelliteLinkError::Closed);
            }
            if let Some(old) = slot.replace(adapter) {
                old.close().await.ok();
            }
        }
        if self.state.load() < TransportState::Available {
            self.set_state(TransportState::Available);
        }
        Ok(())
    }

    /// Terminal unplug: close + drop the adapter; the transport goes
    /// Unavailable (`TransportManager::deregister` removes it from selection).
    pub async fn detach_adapter(&self) {
        if let Some(adapter) = self.adapter.write().await.take() {
            if let Err(e) = adapter.close().await {
                tracing::warn!(
                    target: "iris.transport.satellite",
                    error = %e,
                    "adapter close failed on detach; modem session may be leaked"
                );
                self.metrics.close_failures().fetch_add(1, Ordering::Relaxed);
            }
        }
        if self.state.load() >= TransportState::Available {
            self.set_state(TransportState::Unavailable);
        }
    }

    /// Spend-guard visibility: remaining non-emergency slots this hour.
    pub fn hourly_remaining(&self) -> u32 {
        self.guard.hourly_remaining()
    }

    pub fn metrics_snapshot(&self) -> SatelliteMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Install the discretionary-send confirmation hook (UI wiring point).
    /// Returning `false` denies a P1/P2 send that the guard already admitted.
    pub async fn set_confirm_hook(&self, hook: ConfirmHook) {
        *self.confirm_hook.write().await = hook;
    }

    /// Hold a send for later drain. Rejects payloads the transport could
    /// never carry and bounds depth via the shared backlog cap.
    pub fn enqueue_backlog(&self, msg: SerializedMessage) -> Result<(), TransportError> {
        if msg.payload.len() > MAX_SBD_MO_BYTES {
            // MG-40: resolvable by fragmenting upstream and retrying on
            // this same transport — not a framing/decode failure.
            return Err(TransportError::MessageTooLarge {
                limit: MAX_SBD_MO_BYTES,
                actual: msg.payload.len(),
            });
        }
        if !matches!(
            msg.priority,
            MessagePriority::P0 | MessagePriority::P1 | MessagePriority::P2
        ) {
            // MG-40: permanent for this transport — the caller must
            // re-select, never retry here.
            return Err(TransportError::PolicyDenied("satellite carries P0-P2 only"));
        }
        if !self.backlog.push(msg.priority, msg) {
            return Err(TransportError::Busy);
        }
        Ok(())
    }

    pub fn backlog_len(&self) -> usize {
        self.backlog.len()
    }

    async fn try_send_inner(
        &self,
        peer: &PeerId,
        msg: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError> {
        if self.shutdown_flag.load(Ordering::Acquire) {
            return Err(TransportError::ShuttingDown);
        }
        let adapter = self.adapter.read().await.clone();
        let Some(adapter) = adapter else {
            return Err(TransportError::NotConnected);
        };
        // AC-5: eligibility hard gate — P3+ can never ride satellite. This
        // is MG-1's livelock mechanism (MG-40): the old `Protocol` variant
        // gave the engine no way to tell "permanently ineligible, re-select"
        // from a transient framing glitch, so it retried on the same
        // permanently-ineligible transport until the budget exhausted.
        if !matches!(
            msg.priority,
            MessagePriority::P0 | MessagePriority::P1 | MessagePriority::P2
        ) {
            return Err(TransportError::PolicyDenied(
                "satellite carries P0-P2 emergency traffic only",
            ));
        }
        // SAT-RT-105: gate on the MT budget — mesh-relayed traffic rides the
        // 270-B return leg. MO 340 remains available for gateway egress only.
        if msg.payload.len() > MAX_SBD_MT_BYTES {
            return Err(TransportError::MessageTooLarge {
                limit: MAX_SBD_MT_BYTES,
                actual: msg.payload.len(),
            });
        }
        SbdFrame::new_mo(&msg.payload).map_err(|e| TransportError::Protocol(e.to_string()))?;
        let admission = match self.guard.admit_tx(msg.priority) {
            Ok(a) => a,
            Err(r) => {
                tracing::debug!(
                    target: "iris.transport.satellite",
                    refusal = ?r.reason,
                    "send refused by spend guard; re-route or queue"
                );
                match r.reason {
                    GuardRefusalReason::HourlyCap => {
                        self.metrics
                            .hourly_refusals()
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    GuardRefusalReason::DailyBudget => {
                        self.metrics
                            .daily_refusals()
                            .fetch_add(1, Ordering::Relaxed);
                    }
                }
                return Err(TransportError::Busy);
            }
        };
        if admission.requires_confirmation {
            // SAT-RT-106: a panicking user hook is treated as DENIAL — the
            // admission is refunded below and the task survives. (Sync hook
            // runs on this worker; UI wiring owns real-world latency.)
            let approved = match self.confirm_hook.read().await.as_ref() {
                Some(hook) => {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook(msg.priority)))
                        .unwrap_or(false)
                }
                // RF-20: fail-closed — no hook = no approval. Headless nodes
                // that never need user confirmation must set
                // CostGuardConfig { confirm_discretionary_p1_p2: false }
                // so this branch is never reached (requires_confirmation=false).
                None => false,
            };
            if !approved {
                self.guard.refund_tx(admission.token);
                self.metrics
                    .confirmation_denied()
                    .fetch_add(1, Ordering::Relaxed);
                return Err(TransportError::Busy);
            }
        }
        let tx_result = tokio::time::timeout(TX_TIMEOUT, adapter.tx(&msg.payload))
            .await
            .map_err(|_| SatelliteLinkError::Io("tx timed out".to_string()));
        if let Err(e) = tx_result.and_then(|r| r) {
            // RT-104: the payload never reached the air — refund the spend.
            self.guard.refund_tx(admission.token);
            self.metrics.tx_failures().fetch_add(1, Ordering::Relaxed);
            tracing::debug!(target: "iris.transport.satellite", "tx failed after guard admission; budget refunded");
            return Err(satellite_link_err("satellite link tx", e));
        }
        self.metrics.packets_tx().fetch_add(1, Ordering::Relaxed);
        self.metrics
            .bytes_tx()
            .fetch_add(msg.payload.len() as u64, Ordering::Relaxed);
        tracing::debug!(
            target: "iris.transport.satellite",
            priority = %msg.priority,
            "satellite tx admitted"
        );
        let bytes_sent = msg.payload.len();
        self.ewma.record_send(bytes_sent); // MG-17
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent,
            sent_at: Instant::now(),
        })
    }

    /// Drain up to `max` backlog entries in priority order; entries still
    /// blocked by the guard stay queued (queue-not-drop, AC-4).
    pub async fn drain_backlog(&self, max: usize) -> Vec<(SerializedMessage, SendReceipt)> {
        let mut drained = Vec::new();
        let mut deferred = Vec::new();
        let now = Instant::now();
        'drain: for _ in 0..max {
            let Some(mut entry) = self.backlog.pop_highest() else {
                break;
            };
            // RF-23: skip entries whose retry window has not yet elapsed.
            if now < entry.next_attempt_at {
                deferred.push(entry);
                continue;
            }
            // Backlog entries carry no destination (raw broadcast medium):
            // receipts use the zero-PeerId fallback convention.
            match self.try_send_inner(&PeerId([0u8; 32]), &entry.item).await {
                Ok(receipt) => drained.push((entry.item, receipt)),
                // RF-12: terminal transport errors — put this entry back and
                // stop draining; the transport cannot deliver anything now.
                Err(TransportError::ShuttingDown | TransportError::NotConnected) => {
                    self.backlog.push_deferred(entry);
                    break 'drain;
                }
                // RF-12: permanent per-message errors — drop and count.
                Err(
                    TransportError::Protocol(_)
                    | TransportError::PolicyDenied(_)
                    | TransportError::MessageTooLarge { .. },
                ) => {
                    self.metrics.backlog_protocol_drops().fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(
                        target: "iris.transport.satellite",
                        priority = ?entry.priority,
                        "backlog entry permanently dropped (protocol/policy error)"
                    );
                }
                // RF-23: transient error — schedule retry after RETRY_BASE_MS
                // + up to RETRY_JITTER_FRACTION of additional random delay
                // (AC-9: asynchronous custody-style, never sync-RTT paced).
                Err(_) => {
                    let jitter_range =
                        (RETRY_BASE_MS as f64 * RETRY_JITTER_FRACTION as f64) as u64;
                    let jitter_ms: u64 =
                        rand::Rng::gen_range(&mut rand::thread_rng(), 0..=jitter_range);
                    entry.next_attempt_at =
                        Instant::now() + Duration::from_millis(RETRY_BASE_MS + jitter_ms);
                    deferred.push(entry);
                }
            }
        }
        for entry in deferred {
            // SAT-RT-104: deferred items were already queued once — re-insert
            // over-capacity so concurrent enqueues can never drop held P0-P2
            // traffic (queue-not-drop, AC-4).
            self.backlog.push_deferred(entry);
        }
        drained
    }

    /// Poll the link for inbound MT payloads and forward them onto the
    /// incoming stream. Each call bills one mailbox check (RX costs money —
    /// FC-7). Malformed frames are skipped-and-counted (RT-105), never fatal.
    /// Sender attribution is the zero PeerId fallback (WAW-RT-007 precedent).
    pub async fn poll_inbound(&self) -> Result<usize, TransportError> {
        if self.shutdown_flag.load(Ordering::Acquire) {
            return Err(TransportError::ShuttingDown);
        }
        let adapter = self.adapter.read().await.clone();
        let Some(adapter) = adapter else {
            return Err(TransportError::NotConnected);
        };
        // RF-22: skip this poll if the minimum inter-check interval has not
        // elapsed — prevents a high-frequency caller from exhausting the daily
        // budget on empty mailbox sessions.
        if !self.guard.record_mailbox_check() {
            return Ok(0);
        }
        self.metrics
            .mailbox_checks()
            .fetch_add(1, Ordering::Relaxed);
        let mut delivered = 0usize;
        loop {
            match adapter.rx().await {
                Ok(Some(payload)) => match SbdFrame::new_mt(&payload) {
                    Ok(frame) => {
                        self.metrics.packets_rx().fetch_add(1, Ordering::Relaxed);
                        self.metrics
                            .bytes_rx()
                            .fetch_add(payload.len() as u64, Ordering::Relaxed);
                        self.incoming_tx.send(IncomingMessage {
                            peer_id: PeerId([0u8; 32]),
                            transport_id: self.id.as_str().to_string(),
                            payload: frame.envelope,
                            received_at: Instant::now(),
                        }).ok();
                        delivered += 1;
                    }
                    Err(e) => {
                        self.metrics.malformed_rx().fetch_add(1, Ordering::Relaxed);
                        tracing::debug!(
                            target: "iris.transport.satellite",
                            error = %e,
                            "malformed inbound payload dropped (batch continues)"
                        );
                    }
                },
                Ok(None) => break,
                Err(e) => return Err(satellite_link_err("satellite link rx", e)),
            }
        }
        Ok(delivered)
    }
}

#[async_trait]
impl Transport for SatelliteTransport {
    fn transport_id(&self) -> &TransportId {
        &self.id
    }

    fn display_name(&self) -> &str {
        &self.display
    }

    fn capabilities(&self) -> &TransportCapabilities {
        &self.caps
    }

    fn state(&self) -> TransportState {
        self.state.load()
    }

    fn state_stream(&self) -> Pin<Box<dyn Stream<Item = TransportStateEvent> + Send>> {
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "satellite.state")
    }

    async fn discover_peers(
        &self,
        _config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        // Point-to-point relay path: neighbor discovery happens on the
        // terrestrial legs; satellite carries addressed/broadcast envelopes.
        Ok(Box::pin(futures_util::stream::empty()))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn start_advertising(&self, _info: NodeAdvertisement) -> Result<(), TransportError> {
        Ok(())
    }

    async fn stop_advertising(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn connect(&self, peer: &PeerInfo) -> Result<TransportLink, TransportError> {
        let adapter = self.adapter.read().await.clone();
        let Some(adapter) = adapter else {
            return Err(TransportError::NotConnected);
        };
        // RT-103: probe before promoting — dead/faulted devices fail here.
        adapter
            .status()
            .await
            .map_err(|e| satellite_link_err("satellite status probe", e))?;
        // SAT-RT-108: re-verify the adapter still exists after the probe (a
        // racing detach must not leave a transient Connected-without-link).
        if self.adapter.read().await.is_none() {
            return Err(TransportError::NotConnected);
        }
        self.set_state(TransportState::Connected);
        Ok(TransportLink {
            peer_id: peer.peer_id,
            transport_id: self.id.as_str().to_string(),
            established_at: Instant::now(),
        })
    }

    async fn send(
        &self,
        peer: &PeerId,
        message: &SerializedMessage,
    ) -> Result<SendReceipt, TransportError> {
        // Receipt semantics (recorded): "handed to the radio", NEVER
        // "delivered to peer" — ACK/dedup layers above are authoritative.
        self.try_send_inner(peer, message).await
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        crate::transport::broadcast_stream(self.incoming_tx.subscribe(), "satellite.messages")
    }

    fn cost_snapshot(&self) -> TransportCost {
        // Headroom proxies remaining non-emergency quota this hour.
        let headroom = if self.guard.cfg.hourly_msg_cap == 0 {
            0.02
        } else {
            (self.guard.hourly_remaining() as f32 / self.guard.cfg.hourly_msg_cap as f32)
                .clamp(0.02, 1.0)
        };
        TransportCost {
            estimated_battery_ma: SAT_COST.connected_idle_ma,
            monetary_cost_per_kb: sat_est_cost_per_kb(),
            // MG-17: observed EWMA goodput; fall back to headroom-weighted rate.
            bandwidth_available_bps: self.ewma.bandwidth_bps(
                (SatelliteProvider::IridiumSbd.raw_rate_bps() as f32 * headroom) as u64,
            ),
            congestion_level: 0.0,
        }
    }

    fn set_send_priority_hint(&self, priority: MessagePriority) {
        self.last_priority
            .store(priority.as_u8(), Ordering::Relaxed);
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        self.shutdown_flag.store(true, Ordering::Release);
        // RT-103: take the adapter out so post-shutdown attach cannot
        // resurrect a closed device behind Available state.
        let close_err = if let Some(adapter) = self.adapter.write().await.take() {
            adapter.close().await.err()
        } else {
            None
        };
        self.set_state(TransportState::Unavailable);
        if let Some(e) = close_err {
            self.metrics.close_failures().fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                target: "iris.transport.satellite",
                error = %e,
                "adapter close failed on shutdown"
            );
            return Err(TransportError::Io {
                kind: std::io::ErrorKind::Other,
                msg: e.to_string(),
            });
        }
        Ok(())
    }
}

/// Provisional per-KB estimate derived from per-message pricing (G-5:
/// folklore-grade until the real-modem benchmark lands).
fn sat_est_cost_per_kb() -> f64 {
    SAT_EST_TX_COST_INR / (MAX_SBD_MO_BYTES as f64 / 1024.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::manager::{TransportManager, TransportSelectionRequest};
    use crate::transport::RegistrationError;
    use futures_util::StreamExt;
    use std::sync::atomic::AtomicU64;

    fn msg(priority: MessagePriority, payload_len: usize) -> SerializedMessage {
        use crate::protocol::MessageId;
        SerializedMessage {
            message_id: MessageId::from([7u8; 16]),
            priority,
            payload: vec![0xCD; payload_len],
        }
    }

    fn fake_guard(
        start_ms: u64,
        cfg: CostGuardConfig,
    ) -> (Arc<AtomicU64>, Arc<SatelliteCostGuard>) {
        let clock = Arc::new(AtomicU64::new(start_ms));
        let c = clock.clone();
        let ledger: Arc<dyn CostLedger> = Arc::new(InMemoryLedger::new());
        let guard = Arc::new(SatelliteCostGuard::with_clock(
            cfg,
            ledger,
            Box::new(move || c.load(Ordering::Relaxed)),
        ));
        (clock, guard)
    }

    fn sim_pair() -> (
        Arc<SimulatedSatelliteAdapter>,
        Arc<SimulatedSatelliteAdapter>,
    ) {
        let a = Arc::new(SimulatedSatelliteAdapter::new(SimSatConfig::default()));
        let b = Arc::new(SimulatedSatelliteAdapter::new(SimSatConfig::default()));
        SimulatedSatelliteAdapter::connect_pair(&a, &b);
        (a, b)
    }

    // ---- caps row (AC-1) ----

    #[test]
    fn caps_row_pins_satellite_contract() {
        let t = SatelliteTransport::new("sat-0");
        let caps = t.capabilities();
        assert_eq!(caps.max_message_size, MAX_SBD_MT_BYTES);
        assert_eq!(
            caps.max_message_size, 270,
            "SAT-RT-105: MT relay budget advertised"
        );
        assert!(caps.cost_class.is_expensive());
        assert!(caps.requires_special_hardware);
        assert!(caps.requires_infrastructure);
        assert!(!caps.supports_multicast);
        assert_eq!(caps.typical_latency_ms, 90_000);
        assert!(caps
            .regulatory_band
            .as_deref()
            .is_some_and(|b| b.contains("GMPCS")));
        let cs = t.cost_snapshot();
        assert!(cs.monetary_cost_per_kb > 0.0, "satellite is never free");
        assert!(cs.bandwidth_available_bps <= 2_400);
    }

    #[test]
    fn provider_reserved_variants_documented_non_exhaustive() {
        // Forward-compat: downstream matches must carry a wildcard arm
        // (#[non_exhaustive]); inside the defining crate the set is closed.
        let p = SatelliteProvider::IridiumSbd;
        assert_eq!(p.raw_rate_bps(), 2_400);
    }

    // ---- SBD frame caps + verbatim carry (AC-7) ----

    #[test]
    fn sbd_frame_caps_enforced_mo_mt_asymmetric() {
        // MO: envelope <= 340 verbatim (no on-air IRIS header).
        let ok = SbdFrame::new_mo(&[0u8; 340]);
        assert!(ok.is_ok(), "340-B envelope rides one MO message");
        assert!(matches!(
            SbdFrame::new_mo(&[0u8; 341]),
            Err(SbdError::TooLargeMo(_))
        ));
        // MT: total <= 270 on decode.
        assert!(matches!(
            SbdFrame::new_mt(&vec![0u8; MAX_SBD_MT_BYTES + 1]),
            Err(SbdError::TooLargeMt(_))
        ));
        // Defensive parse at THIS layer: caps + emptiness only — content
        // trust lives in the envelope layer above (DEC-SAT-0002).
        assert!(matches!(SbdFrame::new_mt(&[]), Err(SbdError::Empty)));
    }

    #[test]
    fn sbd_frame_verbatim_byte_equality_all_priorities() {
        for p in 0..=2u8 {
            let priority = MessagePriority::from_u8(p).expect("p0..p2 eligible");
            let envelope = vec![0xCD; 60];
            let _ = priority; // eligibility metadata lives ABOVE the transport
            let frame = SbdFrame::new_mo(&envelope).expect("small envelope wraps");
            let wire = frame.as_bytes();
            assert_eq!(wire, &envelope, "VERBATIM carry: wire bytes == envelope");
            let decoded = SbdFrame::new_mt(wire).expect("round-trips");
            assert_eq!(decoded.envelope, envelope);
            assert_eq!(decoded.direction, SbdDirection::MobileTerminated);
        }
    }
    // ---- cost guard (AC-2/AC-3/AC-4) ----

    #[test]
    fn guard_p0_flows_when_all_budgets_exhausted_ac2() {
        let cfg = CostGuardConfig {
            hourly_msg_cap: 2,
            daily_budget_inr: 20.0,
            ..Default::default()
        };
        let (_clock, guard) = fake_guard(1_000_000, cfg);
        // Exhaust BOTH the hourly cap and the daily budget with P2 traffic.
        guard.admit_tx(MessagePriority::P2).expect("#1");
        guard.admit_tx(MessagePriority::P2).expect("#2");
        assert!(matches!(
            guard.admit_tx(MessagePriority::P2),
            Err(GuardRefusal {
                reason: GuardRefusalReason::HourlyCap,
                ..
            })
        ));
        // SOS-exemption: P0 flows even with zero remaining budget/capacity.
        let admission = guard
            .admit_tx(MessagePriority::P0)
            .expect("P0 is never blocked");
        assert!(!admission.requires_confirmation);
    }

    #[test]
    fn guard_hourly_cap_refuses_then_recovers_after_window() {
        let (_clock, guard) = fake_guard(
            5_000_000,
            CostGuardConfig {
                hourly_msg_cap: 1,
                daily_budget_inr: 1_000.0,
                ..Default::default()
            },
        );
        guard.admit_tx(MessagePriority::P1).expect("#1 fits cap");
        let refusal = guard
            .admit_tx(MessagePriority::P1)
            .expect_err("hourly cap spent");
        assert_eq!(refusal.reason, GuardRefusalReason::HourlyCap);
        assert_eq!(refusal.retry_at_unix_ms - 5_000_000, HOURLY_WINDOW_MS + 1);
    }

    #[test]
    fn guard_daily_budget_blocks_and_resets_next_window() {
        let (clock, guard) = fake_guard(
            86_400_000,
            CostGuardConfig {
                hourly_msg_cap: 1_000,
                daily_budget_inr: 25.0,
                est_cost_per_msg_inr: 10.0,
                confirm_discretionary_p1_p2: false,
                ..Default::default()
            },
        );
        guard.admit_tx(MessagePriority::P1).expect("#1 (10 INR)");
        guard.admit_tx(MessagePriority::P1).expect("#2 (20 INR)");
        let refusal = guard
            .admit_tx(MessagePriority::P1)
            .expect_err("30 > 25 INR daily budget");
        assert_eq!(refusal.reason, GuardRefusalReason::DailyBudget);
        assert_eq!(
            refusal.retry_at_unix_ms - 86_400_000,
            DAY_MS + 1,
            "budget frees at the day boundary"
        );
        // Roll into tomorrow: the day index changes, today's counters reset.
        clock.store(86_400_000 + DAY_MS, Ordering::Relaxed);
        guard
            .admit_tx(MessagePriority::P1)
            .expect("day rolled -> budget reset");
    }

    #[test]
    fn ledger_records_every_mutation_double_write_seam() {
        let a = Arc::new(InMemoryLedger::new());
        let b = Arc::new(InMemoryLedger::new());
        let fanout = FanoutLedger::new(vec![a.clone(), b.clone()]);
        // daily_budget_inr=15.0: first P2 (10 INR = 1000c) fits budget (1500c);
        // second P2 (next=2000c) exceeds it → DailyBudget refusal. Setting
        // min_mailbox_interval_ms=0 so the mailbox check at t=0 actually bills
        // (default=300s throttle would make it a no-op at t=0).
        let (_clock, guard) = fake_guard(
            0,
            CostGuardConfig {
                hourly_msg_cap: 500,
                daily_budget_inr: 15.0,
                confirm_discretionary_p1_p2: false,
                min_mailbox_interval_ms: 0,
                ..Default::default()
            },
        );
        // Re-wire the guard onto the fanout ledger via a second instance.
        let guard2 = SatelliteCostGuard::with_clock(
            CostGuardConfig {
                hourly_msg_cap: 500,
                daily_budget_inr: 10_000.0,
                min_mailbox_interval_ms: 0,
                ..Default::default()
            },
            Arc::new(fanout),
            Box::new(|| 42),
        );
        guard.admit_tx(MessagePriority::P2).expect("admit");
        guard.record_mailbox_check();
        guard.admit_tx(MessagePriority::P2).expect_err("daily cap");
        guard.refund_tx(1); // exact admission token (SAT-RT-103)

        let admission = guard2.admit_tx(MessagePriority::P2).expect("fanout admit");
        guard2.record_mailbox_check();
        assert!(guard2.refund_tx(admission.token), "live token refunds");

        let ea = a.events();
        let eb = b.events();
        assert!(!ea.is_empty());
        assert_eq!(ea.len(), eb.len(), "double-write seam delivers to both");
        assert!(eb.contains(&LedgerEvent::TxAdmitted { emergency: false }));
        assert!(eb.contains(&LedgerEvent::MailboxCheckBilled));
        assert!(eb.contains(&LedgerEvent::TxRefunded));
    }

    #[tokio::test]
    async fn confirmation_hook_gates_discretionary_sends() {
        let t = SatelliteTransport::new("sat-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        t.set_confirm_hook(Some(Arc::new(|p| matches!(p, MessagePriority::P0))))
            .await;

        // Discretionary P2 denied by the hook -> refund + counter.
        let err = t
            .send(&PeerId([1u8; 32]), &msg(MessagePriority::P2, 40))
            .await
            .expect_err("hook denies discretionary P2");
        assert!(matches!(err, TransportError::Busy));
        assert_eq!(t.metrics_snapshot().confirmation_denied, 1);
        assert_eq!(t.metrics_snapshot().packets_tx, 0);

        // Emergency P0 bypasses the hook entirely.
        t.send(&PeerId([1u8; 32]), &msg(MessagePriority::P0, 40))
            .await
            .expect("P0 never waits for confirmation");
        assert_eq!(t.metrics_snapshot().packets_tx, 1);
    }

    // ---- transport integration ----

    #[tokio::test]
    async fn transport_eligibility_gate_rejects_p3_plus_ac5() {
        let t = SatelliteTransport::new("sat-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        for p in [
            MessagePriority::P3,
            MessagePriority::P4,
            MessagePriority::P7,
        ] {
            let err = t
                .send(&PeerId([2u8; 32]), &msg(p, 40))
                .await
                .expect_err("P3+ can never ride satellite");
            assert!(matches!(err, TransportError::PolicyDenied(_)));
        }
        // Backlog enqueues are gated identically.
        assert!(t.enqueue_backlog(msg(MessagePriority::P5, 40)).is_err());
    }

    #[tokio::test]
    async fn transport_rejects_over_mo_payload() {
        let t = SatelliteTransport::new("sat-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        let err = t
            .send(&PeerId([3u8; 32]), &msg(MessagePriority::P0, 271))
            .await
            .expect_err(">270 B cannot ride the MT relay leg");
        assert!(matches!(err, TransportError::MessageTooLarge { .. }));
    }

    #[tokio::test]
    async fn send_consumes_hourly_cap_queue_not_drop_recovery_ac4() {
        let (_clock, guard) = fake_guard(
            0,
            CostGuardConfig {
                hourly_msg_cap: 1,
                daily_budget_inr: 1_000.0,
                confirm_discretionary_p1_p2: false,
                ..Default::default()
            },
        );
        let t = SatelliteTransport::with_guard("sat-0", guard);
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");

        let peer = PeerId([4u8; 32]);
        t.send(&peer, &msg(MessagePriority::P1, 40))
            .await
            .expect("first P1 within cap");
        assert!(t.send(&peer, &msg(MessagePriority::P1, 40)).await.is_err());

        // Queue-not-drop: hold the refused message, drain recovers later.
        t.enqueue_backlog(msg(MessagePriority::P1, 40))
            .expect("fits the queue");
        assert_eq!(t.drain_backlog(4).await.len(), 0, "still capped");
        assert_eq!(t.backlog_len(), 1);

        // Window rolls -> backlog drains.
        // (Clock lives inside the guard's closure; roll via a new hour of
        // wall time is impossible here, so rebuild with an advanced clock.)
        drop(t);
        let (clock2, guard2) = fake_guard(
            HOURLY_WINDOW_MS,
            CostGuardConfig {
                hourly_msg_cap: 1,
                daily_budget_inr: 1_000.0,
                confirm_discretionary_p1_p2: false,
                ..Default::default()
            },
        );
        clock2.store(HOURLY_WINDOW_MS * 2, Ordering::Relaxed);
        let t2 = SatelliteTransport::with_guard("sat-0", guard2);
        let (a2, _b2) = sim_pair();
        t2.attach_adapter(a2).await.expect("adapter opens");
        t2.enqueue_backlog(msg(MessagePriority::P1, 40))
            .expect("re-seeded queue");
        let drained = t2.drain_backlog(4).await;
        assert_eq!(drained.len(), 1, "window rolled -> backlog drains");
        assert_eq!(drained[0].0.priority, MessagePriority::P1);
    }

    #[tokio::test]
    async fn tx_failure_refunds_guard_rt104() {
        let (_clock, guard) = fake_guard(0, CostGuardConfig::default());
        let t = SatelliteTransport::with_guard("sat-0", guard);
        // RF-20: default config has confirm_discretionary_p1_p2=true; install
        // an approving hook so the guard passes and the link error surfaces.
        t.set_confirm_hook(Some(Arc::new(|_| true))).await;
        let (a, _b) = sim_pair();
        t.attach_adapter(a.clone()).await.expect("adapter opens");

        // Kill the link AFTER attach: admission happens, then tx fails ->
        // refund + failure metric, no budget burn.
        a.close().await.expect("close sim link");
        let err = t
            .send(&PeerId([5u8; 32]), &msg(MessagePriority::P1, 40))
            .await
            .expect_err("closed link fails the tx");
        // MG-39/41: a closed link is distinguishable from a genuine I/O
        // failure now — `SatelliteLinkError::Closed` maps to `NotConnected`.
        assert!(matches!(err, TransportError::NotConnected));
        let m = t.metrics_snapshot();
        assert_eq!(m.tx_failures, 1);
        assert_eq!(m.packets_tx, 0);
        assert_eq!(
            t.hourly_remaining(),
            CostGuardConfig::default().hourly_msg_cap,
            "reservation refunded"
        );
    }

    #[tokio::test]
    async fn malformed_inbound_skip_and_count_rt105() {
        let ta = SatelliteTransport::new("sat-a");
        let tb = SatelliteTransport::new("sat-b");
        let (a, b) = sim_pair();
        ta.attach_adapter(a.clone()).await.expect("opens");
        tb.attach_adapter(b.clone()).await.expect("opens");

        // Inject garbage (over-MT blob) then a valid frame toward tb.
        a.tx(&vec![0xFFu8; MAX_SBD_MT_BYTES + 5])
            .await
            .expect("queued");
        let good: Vec<u8> = vec![0x42; 40]; // a verbatim envelope-sized blob
        a.tx(&good).await.expect("good queued");

        // One mailbox check billed per poll; garbage skipped, survivor kept.
        assert_eq!(tb.poll_inbound().await.expect("poll"), 1);
        let m = tb.metrics_snapshot();
        assert_eq!(m.malformed_rx, 1);
        assert_eq!(m.packets_rx, 1);
        assert_eq!(m.mailbox_checks, 1);
    }

    #[tokio::test]
    async fn dead_dongle_attach_rejected_rt103() {
        let t = SatelliteTransport::new("sat-0");
        let gated = AtSbdModemAdapter::new("/dev/ttyUSB0", 115_200);
        let err = t
            .attach_adapter(Arc::new(gated))
            .await
            .expect_err("HardwareGated open must fail attach");
        assert_eq!(err, SatelliteLinkError::HardwareGated);
        assert_eq!(t.state(), TransportState::Unavailable);
    }

    #[tokio::test]
    async fn hot_plug_lifecycle_shutdown_clean() {
        let t = SatelliteTransport::new("sat-0");
        assert_eq!(t.state(), TransportState::Unavailable);

        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("attach");
        assert_eq!(t.state(), TransportState::Available);

        t.detach_adapter().await;
        assert_eq!(t.state(), TransportState::Unavailable);

        let (a2, _b2) = sim_pair();
        t.attach_adapter(a2).await.expect("reattach");
        t.shutdown().await.expect("clean shutdown");
        assert_eq!(t.state(), TransportState::Unavailable);
        assert!(matches!(
            t.send(&PeerId([6u8; 32]), &msg(MessagePriority::P0, 10))
                .await,
            Err(TransportError::ShuttingDown)
        ));
    }

    #[tokio::test]
    async fn end_to_end_pair_delivery_zero_attribution() {
        let ta = SatelliteTransport::new("sat-a");
        let tb = SatelliteTransport::new("sat-b");
        let (a, b) = sim_pair();
        ta.attach_adapter(a).await.expect("opens");
        tb.attach_adapter(b).await.expect("opens");

        let mut inbox = tb.incoming_messages();
        let sent = msg(MessagePriority::P0, 60);
        ta.send(&PeerId([7u8; 32]), &sent)
            .await
            .expect("cross-link");
        assert_eq!(tb.poll_inbound().await.expect("poll"), 1);
        let got = tokio::time::timeout(Duration::from_millis(200), inbox.next())
            .await
            .expect("arrives")
            .expect("stream yields");
        assert_eq!(got.payload, sent.payload);
        assert_eq!(got.transport_id, "sat-b");
        assert_eq!(got.peer_id, PeerId([0u8; 32]), "zero fallback attribution");
    }

    #[tokio::test]
    async fn sim_outage_drops_deterministically_full_loss() {
        let cfg = SimSatConfig {
            outage_probability: 1.0,
            seed: 7,
            ..Default::default()
        };
        let a = SimulatedSatelliteAdapter::new(cfg.clone());
        let b = SimulatedSatelliteAdapter::new(cfg);
        SimulatedSatelliteAdapter::connect_pair(&a, &b);
        a.open().await.unwrap();
        b.open().await.unwrap();
        a.tx(b"lost").await.expect("loss silent at tx time");
        assert!(b.rx().await.unwrap().is_none(), "outage swallowed it");
    }

    #[tokio::test]
    async fn stub_adapters_report_hardware_gated() {
        let at = AtSbdModemAdapter::new("/dev/ttyUSB0", 115_200);
        assert_eq!(
            at.open().await,
            Err(SatelliteLinkError::HardwareGated),
            "BLK-0005 gate"
        );
    }

    #[tokio::test]
    async fn manager_registers_selects_and_hot_unplugs_satellite() {
        let mgr = TransportManager::new();
        let t = Arc::new(SatelliteTransport::new("sat-0"));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");

        mgr.register(t.clone()).await.expect("register");
        assert!(matches!(
            mgr.register(t.clone()).await,
            Err(RegistrationError::DuplicateId)
        ));

        let req_ok = TransportSelectionRequest {
            // MG-14: satellite is unicast-only (supports_broadcast = false),
            // so selection requires a target_peer to consult supports_unicast.
            target_peer: Some(PeerId([0u8; 32])),
            message_size: 200,
            priority: MessagePriority::P1,
            max_latency_ms: None,
            prefer_low_cost: false,
            multipath: false,
            fragmentable: false,
        };
        assert!(mgr
            .select_transports(&req_ok)
            .await
            .iter()
            .any(|r| r.transport_id.as_str() == "sat-0"));

        let req_big = TransportSelectionRequest {
            message_size: 400,
            fragmentable: false,
            ..req_ok
        };
        assert!(!mgr
            .select_transports(&req_big)
            .await
            .iter()
            .any(|r| r.transport_id.as_str() == "sat-0"));

        mgr.deregister(&TransportId::from("sat-0"))
            .await
            .expect("deregister");
        assert_eq!(t.state(), TransportState::Unavailable);
    }

    #[test]
    fn async_ack_constants_ac9() {
        // RF-23: these constants are now actually consumed by drain_backlog's
        // retry scheduler. Verify the values remain in the AC-9-compliant range.
        assert_eq!(RETRY_BASE_MS, 300_000, ">= 5 minutes, never sync-RTT");
        assert!(
            RETRY_JITTER_FRACTION > 0.0 && RETRY_JITTER_FRACTION < 1.0,
            "jitter bounded inside (0,1)"
        );
    }

    #[test]
    fn guard_backward_clock_step_cannot_reset_budgets_rt102() {
        let (clock, guard) = fake_guard(
            DAY_MS * 10 + 1_000,
            CostGuardConfig {
                hourly_msg_cap: 1_000,
                daily_budget_inr: 25.0,
                est_cost_per_msg_inr: 10.0,
                ..Default::default()
            },
        );
        guard.admit_tx(MessagePriority::P1).expect("#1");
        guard.admit_tx(MessagePriority::P1).expect("#2");
        assert!(guard.admit_tx(MessagePriority::P1).is_err(), "daily spent");

        // SAT-RT-102: a BACKWARD wall-clock step must NOT roll the day back
        // (which would restore the exhausted budget).
        clock.store(DAY_MS * 8, Ordering::Relaxed);
        assert!(
            guard.admit_tx(MessagePriority::P1).is_err(),
            "monotonic clamp keeps the exhausted budget"
        );

        // Forward past the real boundary -> normal rollover recovery.
        clock.store(DAY_MS * 11, Ordering::Relaxed);
        guard
            .admit_tx(MessagePriority::P1)
            .expect("legitimate next-day recovery");
    }

    #[test]
    fn guard_refund_exact_token_no_mispair_rt103() {
        let (_clock, guard) = fake_guard(
            0,
            CostGuardConfig {
                hourly_msg_cap: 3,
                daily_budget_inr: 1_000.0,
                ..Default::default()
            },
        );
        let a = guard.admit_tx(MessagePriority::P1).expect("A").token;
        let b = guard.admit_tx(MessagePriority::P1).expect("B").token;
        assert_ne!(a, b, "tokens are unique");

        // Refund B by exact token: only B's slot is released.
        assert!(guard.refund_tx(b));
        assert_eq!(guard.hourly_remaining(), 2, "A still holds its slot");

        // Duplicate/late refund of the same token is a safe no-op.
        assert!(!guard.refund_tx(b));

        // Refund A: window fully free again.
        assert!(guard.refund_tx(a));
        assert_eq!(guard.hourly_remaining(), 3);

        // Unknown token: never recorded as refunded.
        assert!(!guard.refund_tx(999));
    }

    #[tokio::test]
    async fn confirmation_hook_panic_treated_as_denial_rt106() {
        let t = SatelliteTransport::new("sat-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        t.set_confirm_hook(Some(Arc::new(|_p| panic!("ui bug"))))
            .await;

        // The panicking hook must not poison the task nor leak the admission.
        let err = t
            .send(&PeerId([8u8; 32]), &msg(MessagePriority::P2, 40))
            .await
            .expect_err("panic-as-denial blocks discretionary send");
        assert!(matches!(err, TransportError::Busy));
        let m = t.metrics_snapshot();
        assert_eq!(m.confirmation_denied, 1);
        assert_eq!(m.tx_failures, 0);
        assert_eq!(
            t.hourly_remaining(),
            CostGuardConfig::default().hourly_msg_cap,
            "admission refunded after hook panic"
        );

        // Emergency path unaffected.
        t.send(&PeerId([8u8; 32]), &msg(MessagePriority::P0, 40))
            .await
            .expect("p0 unaffected");
    }
}
