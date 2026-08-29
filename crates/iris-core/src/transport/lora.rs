//! LoRa long-range transport — LORA-001.
//!
//! Implements the LoRa gateway transport as an **adapter-injected** node (the
//! BLE-001/BLE-002 `BleAdapter`-seam lesson): a platform-neutral Rust core plus
//! a [`LoRaLinkAdapter`] trait whose implementations talk to the external LoRa
//! module over AT-serial (EBYTE E22-900M30S / USB-CDC) or SPI-native (Waveshare
//! SX1262 HAT on RPi) firmware. The physical SX1262/E22 firmware + GATT-bridge
//! packaging is hardware-gated (BLK-0005 / GAP-004 / RES-0027 G-2).
//!
//! Regulatory floor = **WPC G.S.R. 853(E) 2021 Table-I**: 865–868 MHz,
//! **≤25 mW e.r.p. (≈14 dBm)**, **≤1% duty** (36 s / device / hour), applied to
//! the **whole transmission** (airtime, not payload bytes). [`ComplianceConfig`]
//! pins the field config; the [`DutyCycleTracker`] is a hard legal floor with
//! **no runtime override** (Meshtastic `override_duty_cycle_limit` deliberately
//! NOT followed). P0 switches to the next-best transport on exhaustion — never
//! held, never overrides the budget ([`LoRaTransport::duty_cycle_remaining`]
//! is the routing seam). LoRaWAN/IN865 (30 dBm EIRP) is a network-planning
//! figure for a future alternate uplink only (DEC-LORA-0006).
//!
//! v1 = **raw-LoRa P2P** (no LoRaWAN stack, keys, or join ceremony). Default
//! radio profile **SF9 / BW125 / CR4/5**; **P0 uses SF12** (−129/−136 dBm
//! sensitivity, AN1200.13 → ≈+7 dB link budget). No crypto changes: the
//! envelope X25519+ChaCha20-Poly1305 (CRYPTO-001) is transported unchanged
//! (DEC-LORA-0008).
//!
//! All simulated / link-budget evidence is **SIMULATION_VALIDATED** only
//! (BLK-0005 / GAP-004 / RES-0027 G-2/G-3): no field claims (DEC-LORA-0007).

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

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::protocol::MessageId;
use crate::transport::{
    AtomicState, Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportState, TransportStateEvent, LORA_COST,
};
use crate::TransportError;

// ==== 1. Regulatory compliance (RES-0027 RQ-1 / D-7, DEC-LORA-0001) ====

/// Static regulatory rule set consumed by the [`DutyCycleTracker`].
///
/// Pins the field config per WPC G.S.R. 853(E) 2021 Table-I (L1 primary
/// source: thc.nic.in/G25977.pdf): 866.0 MHz, ≤25 mW e.r.p. (≈14 dBm), ≤1%
/// duty over a rolling 1-h window (36,000 ms per device-hour). Airtime counts
/// the whole packet on-air time at SF/BW/CR — not payload bytes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComplianceConfig {
    /// Center frequency (Hz). Field pin: 866.0 MHz inside the 865–868 band.
    pub freq_hz: u64,
    /// Maximum radiated power (dBm e.r.p.). Table-I pin: 14 (= 25 mW).
    pub max_erp_dbm: i8,
    /// Duty-cycle limit (%). Table-I pin: 1.0.
    pub duty_percent: f32,
    /// Rolling window length (ms). Pin: 3,600,000 (1 h).
    pub window_ms: u64,
    /// Airtime budget per window (ms). Derived: `window_ms × duty%`.
    pub airtime_budget_ms: u64,
    /// RF-7: maximum on-time for a single transmission (ms). EN 300 220's
    /// duty-cycle band access conditions the 1% sub-bands typically pair
    /// the aggregate duty figure with a per-transmission ceiling (commonly
    /// 1 s) — the aggregate budget alone does not bound one continuous
    /// emission, which a single SF12 P0 frame could otherwise turn into a
    /// 9.02 s unbroken transmission (see `airtime_matches_semtech_
    /// formula_anchors`).
    ///
    /// **Deliberately NOT the finding's suggested default of 1000.**
    /// Verified independently (a standalone rustc smoke test of the exact
    /// `airtime_ms` formula, before touching this default): at SF12 (the
    /// P0 profile), even a ZERO-payload frame — just the 18-byte header,
    /// the physical minimum possible — already takes 1319 ms. A 1000 ms
    /// cap would make P0 (emergency SOS) categorically unable to transmit
    /// over LoRa at all, under any payload size, which is a far worse
    /// outcome than the finding's own 9.02 s-emission concern — the
    /// finding's own remediation text ("for P0 this forces fragmentation
    /// to ≤1 s frames") assumes fragmenting smaller can bring SF12 under
    /// 1 s, which is not physically possible for this profile. Defaulted
    /// instead to 4000 — comfortably above 3285 ms (P0's real current
    /// worst case: a 60-byte payload, RF-10's own cap, at SF12), so this
    /// does not regress P0 sending, while still meaningfully bounding the
    /// old, uncapped 9020 ms worst case the finding was actually
    /// concerned about. Like `min_off_ms` below, this is a safe,
    /// non-breaking starting point, not a verified EN 300 220 sub-band
    /// figure — tune once that figure is confirmed for this deployment's
    /// exact band/region, and reconcile against `max_payload_for`'s P0
    /// cap (RF-10) if either changes.
    pub max_ton_ms: u64,
    /// RF-7: minimum time between the end of one transmission and the
    /// start of the next (ms). Default 0 (disabled) — unlike
    /// `max_ton_ms`, no specific EN 300 220 off-time figure was given for
    /// this deployment's exact sub-band, and inventing one would risk
    /// encoding a wrong regulatory number rather than no constraint at
    /// all. The mechanism is fully wired (`check_transmission_timing`);
    /// set a nonzero value here once the correct figure for this
    /// deployment's specific band/region is confirmed.
    pub min_off_ms: u64,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        ComplianceConfig {
            // RF-8: IN865_1 per SPECTRUM_CONSIDERATIONS.md §2.1 (865.0625 MHz)
            freq_hz: 865_062_500,
            max_erp_dbm: 14,
            duty_percent: 1.0,
            window_ms: 3_600_000,
            airtime_budget_ms: 36_000,
            max_ton_ms: 4_000,
            min_off_ms: 0,
        }
    }
}

impl ComplianceConfig {
    /// Validate against WPC G.S.R. 853(E) 2021 Table-I limits. The tracker
    /// constructor refuses non-compliant configs — there is no override knob.
    pub fn validate(&self) -> Result<(), ComplianceError> {
        if !(865_000_000..=868_000_000).contains(&self.freq_hz) {
            return Err(ComplianceError::BandOutsideSrd(self.freq_hz));
        }
        if self.max_erp_dbm > 14 {
            return Err(ComplianceError::ErpAboveTableI(self.max_erp_dbm));
        }
        if !(self.duty_percent > 0.0 && self.duty_percent <= 1.0) {
            return Err(ComplianceError::DutyAboveTableI);
        }
        let expected = (self.window_ms as f32 * self.duty_percent / 100.0) as u64;
        if expected.abs_diff(self.airtime_budget_ms) > 1 {
            return Err(ComplianceError::BudgetMismatch {
                expected,
                actual: self.airtime_budget_ms,
            });
        }
        Ok(())
    }
}

/// Why a [`ComplianceConfig`] was rejected: it exceeds a Table-I limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComplianceError {
    BandOutsideSrd(u64),
    ErpAboveTableI(i8),
    DutyAboveTableI,
    BudgetMismatch { expected: u64, actual: u64 },
}

impl std::fmt::Display for ComplianceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComplianceError::BandOutsideSrd(hz) => {
                write!(f, "freq {hz} Hz outside WPC SRD band 865-868 MHz")
            }
            ComplianceError::ErpAboveTableI(dbm) => {
                write!(f, "{dbm} dBm e.r.p. exceeds Table-I limit 14 dBm (25 mW)")
            }
            ComplianceError::DutyAboveTableI => write!(f, "duty cycle exceeds Table-I limit 1%"),
            ComplianceError::BudgetMismatch { expected, actual } => {
                write!(
                    f,
                    "airtime budget {actual} ms != window x duty ({expected} ms)"
                )
            }
        }
    }
}

impl std::error::Error for ComplianceError {}

/// Priority bucket for proportional airtime under contention
/// (REQ-ROUTE-C-002: P0 60% / P1 25% / P2 10% / P3–P7 share 5%).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DutyBucket {
    Emergency,
    Medical,
    Location,
    Bulk,
}

impl DutyBucket {
    pub fn from_priority(p: MessagePriority) -> DutyBucket {
        match p {
            MessagePriority::P0 => DutyBucket::Emergency,
            MessagePriority::P1 => DutyBucket::Medical,
            MessagePriority::P2 => DutyBucket::Location,
            _ => DutyBucket::Bulk,
        }
    }

    /// Airtime share of the hourly budget under contention.
    pub fn budget_fraction(self) -> f32 {
        match self {
            DutyBucket::Emergency => 0.60,
            DutyBucket::Medical => 0.25,
            DutyBucket::Location => 0.10,
            DutyBucket::Bulk => 0.05,
        }
    }

    /// Absolute cap (ms) derived from the device budget.
    pub fn cap_ms(self, airtime_budget_ms: u64) -> u64 {
        (airtime_budget_ms as f32 * self.budget_fraction()) as u64
    }
}

fn bucket_index(b: DutyBucket) -> usize {
    match b {
        DutyBucket::Emergency => 0,
        DutyBucket::Medical => 1,
        DutyBucket::Location => 2,
        DutyBucket::Bulk => 3,
    }
}

/// The next legal send time produced by [`DutyCycleTracker::check_and_consume`].
///
/// Timeline is UNIX milliseconds (injectable clock) so routers can schedule
/// around the window per RES-0008 R7 (`next_send_window`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NextWindow {
    pub delay_ms: u64,
    pub remaining_fraction: f32,
    /// RF-4/RF-16: exact-token refund handle for this reservation (mirrors
    /// `satellite.rs`'s `GuardAdmission::token`, SAT-RT-103). Refunding by
    /// `(class, airtime_ms)` let any code holding the tracker's `Arc`
    /// de-bill an enumerable, guessable key — arithmetically identical to
    /// raising the "no runtime override" legal budget. 0 is never minted
    /// (tokens start at 1), so it is available as an always-invalid sentinel.
    pub token: u64,
}

/// Why a LoRa send was refused: the 1% duty budget (or this bucket's share)
/// is exhausted this window. Carries the next legal send time so the routing
/// layer can schedule around it (RFC 9171 §6.9 analogy, RES-0008 R7) — never
/// a panic, never a hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetExhausted {
    pub delay_ms: u64,
    pub remaining_fraction: f32,
}

/// RF-7: why [`DutyCycleTracker::check_transmission_timing`] refused a send
/// — a distinct, `BudgetExhausted`-sibling error, not the aggregate 1%
/// window budget. `BudgetExhausted` answers "has this device transmitted
/// too much total airtime this hour"; this answers "is THIS ONE
/// transmission itself too long, or too soon after the last one" — EN 300
/// 220's per-transmission timing constraints, checked independently of and
/// before the aggregate budget (there is no point reserving airtime for a
/// transmission that can never legally happen as shaped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimingRefusalReason {
    /// The requested airtime exceeds `ComplianceConfig::max_ton_ms` — no
    /// wait fixes this, the caller must fragment the message smaller.
    MaxOnTimeExceeded,
    /// The send would start before `ComplianceConfig::min_off_ms` has
    /// elapsed since the end of the last transmission.
    MinOffTimeNotElapsed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransmissionTimingRefused {
    pub reason: TimingRefusalReason,
    /// Milliseconds until this exact request would become legal. Always 0
    /// for `MaxOnTimeExceeded` (waiting never helps); the remaining
    /// off-time for `MinOffTimeNotElapsed`.
    pub delay_ms: u64,
}

type NowFn = Box<dyn Fn() -> u64 + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct TxRecord {
    start_ms: u64,
    airtime_ms: u64,
    bucket: DutyBucket,
    /// RF-4/RF-16: exact-token refund handle — see `NextWindow::token`.
    token: u64,
}

fn usage_locked(q: &VecDeque<TxRecord>) -> (u64, [u64; 4]) {
    let mut used = 0u64;
    let mut by_bucket = [0u64; 4];
    for r in q {
        used += r.airtime_ms;
        by_bucket[bucket_index(r.bucket)] += r.airtime_ms;
    }
    (used, by_bucket)
}

fn remaining_fraction(used_ms: u64, budget_ms: u64) -> f32 {
    (1.0 - used_ms as f32 / budget_ms as f32).clamp(0.0, 1.0)
}

/// Sliding 1-h window, 36,000 ms airtime budget, global per-device (all
/// 865–868 MHz TX), **no runtime override** (DEC-LORA-0002).
///
/// Under contention each priority bucket is additionally capped to its
/// proportional share (60/25/10/5). Both the global cap and the bucket cap
/// are hard floors: exceeding either refuses the send and returns the next
/// legal window. There is deliberately no API to raise the budget — the only
/// constructor validates against Table-I ([`ComplianceConfig::validate`]).
pub struct DutyCycleTracker {
    cfg: ComplianceConfig,
    tx: Mutex<VecDeque<TxRecord>>,
    now_fn: NowFn,
    /// RT-102: monotonic clamp — a backward wall-clock step (NTP correction,
    /// manual set) must never make records look younger and freeze the
    /// budget. Accepted time never decreases.
    last_ms: AtomicU64,
    /// Forward-jump guard. `None` when an explicit clock is injected.
    monotonic: Option<Mutex<MonotonicGuard>>,
    /// RF-4/RF-16: mints unique refund tokens (SAT-RT-103 pattern). Starts
    /// at 1 so 0 stays available as an always-invalid sentinel.
    next_token: AtomicU64,
}

/// Bounds wall-clock advancement by real elapsed time.
///
/// The duty-cycle window is a legal limit, so the clock that ages it must not
/// be something an attacker (or a misconfigured NTP client) can fast-forward.
#[derive(Debug)]
struct MonotonicGuard {
    anchor: std::time::Instant,
    /// Wall-clock value last returned.
    last_out: u64,
    /// Elapsed-at-last-observation, so successive calls measure real time.
    last_elapsed_ms: u64,
}

impl MonotonicGuard {
    fn new() -> Self {
        MonotonicGuard {
            anchor: std::time::Instant::now(),
            last_out: 0,
            last_elapsed_ms: 0,
        }
    }

    /// Clamp `raw` to at most "previous output + real time since then".
    fn bound(&mut self, raw: u64) -> u64 {
        let elapsed = self.anchor.elapsed().as_millis() as u64;
        if self.last_out == 0 {
            // First observation establishes the baseline; nothing to compare to.
            self.last_out = raw;
            self.last_elapsed_ms = elapsed;
            return raw;
        }
        let real_delta = elapsed.saturating_sub(self.last_elapsed_ms);
        // A small tolerance absorbs scheduling jitter without permitting a jump.
        let ceiling = self
            .last_out
            .saturating_add(real_delta)
            .saturating_add(MONOTONIC_TOLERANCE_MS);
        let out = raw.min(ceiling).max(self.last_out);
        self.last_out = out;
        self.last_elapsed_ms = elapsed;
        out
    }
}

/// Jitter allowance for the forward-jump guard.
const MONOTONIC_TOLERANCE_MS: u64 = 1_000;

impl DutyCycleTracker {
    /// Tracker pinned to the Table-I default field config (866.0 MHz /
    /// ≤25 mW / ≤1% duty / 36,000 ms per rolling hour).
    ///
    /// # Panics
    /// Never — the default config is compliant by construction.
    pub fn new() -> Self {
        Self::try_new(ComplianceConfig::default()).expect("default config is Table-I compliant")
    }

    /// Tracker from an explicit rule set; rejected unless Table-I compliant.
    pub fn try_new(cfg: ComplianceConfig) -> Result<Self, ComplianceError> {
        cfg.validate()?;
        let now_fn: NowFn = Box::new(system_now_ms);
        Ok(DutyCycleTracker {
            cfg,
            tx: Mutex::new(VecDeque::new()),
            now_fn,
            last_ms: AtomicU64::new(0),
            // Real wall clock => guard against forward jumps.
            monotonic: Some(Mutex::new(MonotonicGuard::new())),
            next_token: AtomicU64::new(1),
        })
    }

    /// Tracker with an injectable millisecond clock (deterministic tests).
    ///
    /// The monotonic guard is disabled here: an injected clock IS the authority
    /// under test, and bounding it by real elapsed time would make every
    /// time-travel test a no-op.
    pub fn with_clock(cfg: ComplianceConfig, now_fn: NowFn) -> Result<Self, ComplianceError> {
        cfg.validate()?;
        Ok(DutyCycleTracker {
            cfg,
            tx: Mutex::new(VecDeque::new()),
            now_fn,
            last_ms: AtomicU64::new(0),
            monotonic: None,
            next_token: AtomicU64::new(1),
        })
    }

    /// Monotonic-clamped current time.
    ///
    /// A backward wall-clock step is ignored (records keep aging on the
    /// pre-jump timeline; RT-102), and a FORWARD step is bounded by the time
    /// that has actually elapsed.
    ///
    /// Only the backward clamp existed before. A forward jump of >= 1 h — an
    /// NTP correction, a hostile time source, or simply setting the device
    /// clock — made `prune_locked` drop every transmission record, and the
    /// backward clamp then wrote the jumped time into `last_ms` so it could
    /// never be walked back. That is an unlimited, repeatable evasion of a
    /// duty-cycle limit the module documents as "a hard legal floor with no
    /// runtime override".
    fn now(&self) -> u64 {
        let raw = (self.now_fn)();
        let t = match &self.monotonic {
            None => raw,
            Some(guard) => {
                let mut g = guard.lock().unwrap_or_else(|p| p.into_inner());
                g.bound(raw)
            }
        };
        let mut prev = self.last_ms.load(Ordering::Acquire);
        while t > prev {
            match self
                .last_ms
                .compare_exchange_weak(prev, t, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => break,
                Err(actual) => prev = actual,
            }
        }
        self.last_ms.load(Ordering::Acquire)
    }

    /// RT-104/RF-4/RF-16: refund a reservation that never reached the air
    /// (TX failed after `check_and_consume` admitted it). `pub(crate)` and
    /// exact-token, not `pub` and (class, airtime) — the module's headline
    /// property is "a hard legal floor with no runtime override", and
    /// `airtime_ms` is deterministic and enumerable from `airtime_ms(profile,
    /// len)`, so a public, guessable-key refund was a one-call bypass of
    /// that property for any code holding the tracker's `Arc` (which
    /// `LoRaTransport::with_tracker` requires the caller to construct).
    /// Mirrors `SatelliteCostGuard::refund_tx`'s token pattern (SAT-RT-103):
    /// `token == 0` (never minted) and an already-refunded/expired token are
    /// both safe no-ops, matching that function's documented behaviour.
    pub(crate) fn refund(&self, token: u64) -> bool {
        if token == 0 {
            return false;
        }
        let mut q = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(idx) = q.iter().position(|r| r.token == token) {
            q.remove(idx);
            return true;
        }
        false
    }

    /// Fraction of the per-device hourly budget still unused, in [0, 1].
    pub fn duty_cycle_remaining_fraction(&self) -> f32 {
        let mut q = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        prune_locked(&mut q, self.cfg.window_ms, self.now());
        let (used, _) = usage_locked(&q);
        remaining_fraction(used, self.cfg.airtime_budget_ms)
    }

    /// Reserve `airtime_ms` for `class`, enforcing the global 36,000 ms/hour
    /// budget AND the priority-proportional bucket cap (60/25/10/5).
    ///
    /// On success the reservation is recorded and the returned [`NextWindow`]
    /// says when the channel frees (this TX's airtime). On refusal the caller
    /// gets [`BudgetExhausted`] with the earliest time a send of this size
    /// becomes legal — schedule around it, re-route, or (for P0) switch
    /// transports. Never panics; never bypasses the budget.
    pub fn check_and_consume(
        &self,
        class: MessagePriority,
        airtime_ms: u64,
    ) -> Result<NextWindow, BudgetExhausted> {
        let now = self.now();
        let window = self.cfg.window_ms;
        let budget = self.cfg.airtime_budget_ms;
        let bucket = DutyBucket::from_priority(class);

        let mut q = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        prune_locked(&mut q, window, now);
        let (used, by_bucket) = usage_locked(&q);
        let global_remaining = budget.saturating_sub(used);
        let bi = bucket_index(bucket);
        let cap = bucket.cap_ms(budget);
        let bucket_remaining = cap.saturating_sub(by_bucket[bi]);

        // RF-3: work-conserving shares. The 60/25/10/5 splits are a fairness
        // policy "under contention" (LORA.md §Duty Cycle, REQ-ROUTE-C-002).
        // Only enforce the bucket cap when lower-priority demand actually sits
        // in the window; otherwise let the bucket borrow unused spectrum from
        // lower-priority slots without exceeding the absolute legal cap.
        let contended = q.iter().any(|r| bucket_index(r.bucket) > bi);
        let effective_remaining = if contended { bucket_remaining } else { global_remaining };

        if airtime_ms <= global_remaining && airtime_ms <= effective_remaining {
            let token = self.next_token.fetch_add(1, Ordering::Relaxed);
            q.push_back(TxRecord {
                start_ms: now,
                airtime_ms,
                bucket,
                token,
            });
            return Ok(NextWindow {
                delay_ms: airtime_ms,
                remaining_fraction: remaining_fraction(used + airtime_ms, budget),
                token,
            });
        }

        // RT-107: find the earliest record expiry that satisfies the binding
        // constraints (walking every record — an absolute-oversize request
        // finds none and waits out the full window). When not contended only
        // global budget recovery matters; when contended both global and bucket
        // must recover (conservative: ignores mid-walk contention changes).
        let mut g_left = global_remaining;
        let mut b_left = bucket_remaining;
        let mut chosen: Option<u64> = None;
        for r in q.iter() {
            let expires = r.start_ms + window;
            g_left += r.airtime_ms;
            if bucket_index(r.bucket) == bi {
                b_left += r.airtime_ms;
            }
            let global_ok = airtime_ms <= g_left;
            let bucket_ok = !contended || airtime_ms <= b_left;
            if global_ok && bucket_ok {
                chosen = Some(expires + 1 - now);
                break;
            }
        }
        let delay = chosen.unwrap_or(window + 1);
        Err(BudgetExhausted {
            delay_ms: delay,
            remaining_fraction: remaining_fraction(used, budget),
        })
    }

    /// RF-7: per-transmission timing constraints, independent of the
    /// aggregate window budget `check_and_consume` enforces. Callers
    /// should check this FIRST — there is no point reserving airtime
    /// (which mutates tracker state) for a transmission that can never
    /// legally happen as shaped, or that must wait regardless of budget.
    pub fn check_transmission_timing(
        &self,
        airtime_ms: u64,
    ) -> Result<(), TransmissionTimingRefused> {
        if airtime_ms > self.cfg.max_ton_ms {
            return Err(TransmissionTimingRefused {
                reason: TimingRefusalReason::MaxOnTimeExceeded,
                delay_ms: 0,
            });
        }
        if self.cfg.min_off_ms > 0 {
            let now = self.now();
            let q = self.tx.lock().unwrap_or_else(|p| p.into_inner());
            // The queue is append-only-at-the-back / prune-from-the-front
            // under a monotonic clock, so it stays ordered by start_ms —
            // `.back()` is the most recent transmission recorded.
            if let Some(last) = q.back() {
                let last_end = last.start_ms.saturating_add(last.airtime_ms);
                let elapsed = now.saturating_sub(last_end);
                if elapsed < self.cfg.min_off_ms {
                    return Err(TransmissionTimingRefused {
                        reason: TimingRefusalReason::MinOffTimeNotElapsed,
                        delay_ms: self.cfg.min_off_ms - elapsed,
                    });
                }
            }
        }
        Ok(())
    }

    /// Read-only access to the compliance configuration this tracker was
    /// constructed with (RF-1: `attach_adapter` needs `freq_hz`/
    /// `max_erp_dbm` to validate an attached module's reported config).
    /// `ComplianceConfig` is `Copy`, so this is a cheap snapshot, not a
    /// live reference into tracker state.
    pub fn compliance_config(&self) -> ComplianceConfig {
        self.cfg
    }
}

impl Default for DutyCycleTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Drop records that have aged out of the sliding window (oldest first).
fn prune_locked(q: &mut VecDeque<TxRecord>, window_ms: u64, now_ms: u64) {
    while let Some(front) = q.front() {
        if now_ms.saturating_sub(front.start_ms) >= window_ms {
            q.pop_front();
        } else {
            break;
        }
    }
}

fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ==== 2. Radio profiles + airtime (RES-0027 RQ-3 / D-4, DEC-LORA-0004) ====

/// LoRa PHY parameters for one link class. Default link = SF9/BW125/CR4/5;
/// P0 = SF12 (+≈7 dB sensitivity at ≈4× airtime — acceptable because P0 is
/// rare and P0-switch protects it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RadioProfile {
    /// Spreading factor 7..=12.
    pub sf: u8,
    /// Channel bandwidth in kHz (125 typical; 250/500 gateway options).
    pub bandwidth_khz: u32,
    /// Coding rate denominator: 5 = CR4/5 .. 8 = CR4/8.
    pub coding_rate_denom: u8,
    /// Center frequency (Hz).
    pub freq_hz: u64,
    /// Preamble symbol count (typical 8).
    pub preamble_symbols: u8,
}

/// Default link profile: SF9 / BW125 kHz / CR4/5 @ IN865_1 (865.0625 MHz).
pub const DEFAULT_RADIO_PROFILE: RadioProfile = RadioProfile {
    sf: 9,
    bandwidth_khz: 125,
    coding_rate_denom: 5,
    // RF-8: IN865_1 per SPECTRUM_CONSIDERATIONS.md §2.1 (865.0625 MHz)
    freq_hz: 865_062_500,
    preamble_symbols: 8,
};

/// P0 emergency profile: SF12 / BW125 kHz / CR4/5 (−136 dBm sensitivity,
/// AN1200.13). Same band; ≈4× the airtime of SF9.
pub const P0_RADIO_PROFILE: RadioProfile = RadioProfile {
    sf: 12,
    bandwidth_khz: 125,
    coding_rate_denom: 5,
    // RF-8: IN865_1 per SPECTRUM_CONSIDERATIONS.md §2.1 (865.0625 MHz)
    freq_hz: 865_062_500,
    preamble_symbols: 8,
};

/// Profile selection: P0 always rides SF12; everything else the default.
pub fn profile_for_priority(p: MessagePriority) -> RadioProfile {
    if matches!(p, MessagePriority::P0) {
        P0_RADIO_PROFILE
    } else {
        DEFAULT_RADIO_PROFILE
    }
}

/// Raw on-air bit rate (bps) for a profile: `SF · BW/2^SF · CR`.
pub fn raw_rate_bps(profile: &RadioProfile) -> u64 {
    let bw_hz = profile.bandwidth_khz as f64 * 1000.0;
    let symbol_rate = bw_hz / (2u64.pow(profile.sf as u32) as f64);
    (profile.sf as f64 * symbol_rate * 4.0 / profile.coding_rate_denom as f64) as u64
}

/// Whole-packet on-air time (ms, rounded UP — never under-count legal
/// airtime) per the Semtech AN1200.13 formula:
///
/// ```text
/// Tsym   = 2^SF / BW
/// Nbit   = 8·PL − 4·SF + 28 + 16·CRC − 20·IH      (CRC=1, IH=0 here)
/// Nsym   = 8 + max(ceil(Nbit / (4·(SF − 2·DE))), 0) · (CR + 4)
/// DE     = 1 when SF >= 11 && BW == 125 kHz (low-data-rate optimization)
/// Airtime = (preamble + 4.25 + Nsym) · Tsym
/// ```
///
/// The 4.25 term is the preamble's half-symbol tail; the +8 is header/sync.
/// `payload_len` must be the FULL on-air frame length (`HEADER_LEN +
/// envelope`), not just the envelope — RT-101.
///
/// # Panics (debug builds only, RT-109)
/// debug_asserts the profile fields are physically meaningful; release
/// builds saturate fail-closed (degenerate profiles yield huge airtime that
/// the duty tracker refuses).
pub fn airtime_ms(profile: &RadioProfile, payload_len: usize) -> u64 {
    debug_assert!(
        (7..=12).contains(&profile.sf)
            && matches!(profile.bandwidth_khz, 125 | 250 | 500)
            && (5..=8).contains(&profile.coding_rate_denom),
        "RadioProfile outside LoRa PHY range (RT-109)"
    );
    let sf = profile.sf as f64;
    let bw_hz = profile.bandwidth_khz as f64 * 1000.0;
    let ts_ms = 2.0f64.powf(sf) / bw_hz * 1000.0;
    // RF-14: LDRO's physical trigger is symbol time > 16 ms (Semtech
    // AN1200.13), not "SF>=11 && BW==125" — that shorthand covers the
    // default plan but misses e.g. SF12/BW250 (also 16.384 ms), which used
    // to under-count airtime by omitting the DE term. This module's own
    // rounding convention is deliberately conservative everywhere else
    // (`.ceil()` below, "rounded UP — never under-count legal airtime");
    // computing DE from the already-derived ts_ms keeps this case
    // consistent with that convention instead of being the one exception.
    let de = if ts_ms > 16.0 { 1.0 } else { 0.0 };
    let numerator = 8.0 * payload_len as f64 - 4.0 * sf + 28.0 + 16.0;
    let denominator = 4.0 * (sf - 2.0 * de);
    let ceil_term = (numerator / denominator).ceil().max(0.0);
    let payload_symbols = 8.0 + ceil_term * f64::from(profile.coding_rate_denom);
    let total_symbols = f64::from(profile.preamble_symbols) + 4.25 + payload_symbols;
    (total_symbols * ts_ms).ceil().max(1.0) as u64
}

// ==== 3. Frame format + SLIP framing (RES-0027 RQ-4 / D-3, DEC-LORA-0005) ====

/// Wire version of [`LoRaFrame`].
pub const FRAME_VERSION: u8 = 1;
/// ver(1) + prio(1) + msg_id(16). Sized so the design-pinned 237-B V001
/// emergency envelope fits a 255-B on-air frame exactly (RES-0003 /
/// P0_MAX_ENVELOPE_BYTES): 18 + 237 = 255.
pub const HEADER_LEN: usize = 18;
/// Raw-LoRa single-frame cap (RES-0003; the PHY/wire limit).
pub const MAX_FRAME_BYTES: usize = 255;
/// Envelope payload ceiling once the 18-B header sits inside 255 B.
pub const MAX_PAYLOAD_BYTES: usize = MAX_FRAME_BYTES - HEADER_LEN;
/// RT-106: backlog depth bound (memory guard for constrained devices).
pub const MAX_BACKLOG_ENTRIES: usize = 256;

/// RF-9: per-call inbound frame budget, mirroring `wifi_direct::MAX_FRAMES_PER_TICK`.
/// A continuous emitter on the open LoRa medium can keep `rx()` returning
/// `Some` indefinitely — capping the loop lets the poller task return and
/// re-schedule, preventing a livelock.
pub const MAX_FRAMES_PER_POLL: usize = 8;

/// RF-10: LORA.md §Bandwidth Budget's per-priority payload allowance under
/// the ~1.4 B/s effective throughput the 1% duty ceiling leaves — P0 SOS
/// 60 B, P1 Evacuation 80 B, P2 Medical 120 B, P3 Logistics 150 B, and
/// "P4+ never transmitted over LoRa" (`None`). Distinct from
/// [`MAX_PAYLOAD_BYTES`] (237 B, the raw PHY/wire limit every class used
/// to share): that let a 237-byte P6 fragment burn 3.5% of the device's
/// entire legal hour on a class the spec forbids outright.
pub fn max_payload_for(priority: MessagePriority) -> Option<usize> {
    match priority {
        MessagePriority::P0 => Some(60),
        MessagePriority::P1 => Some(80),
        MessagePriority::P2 => Some(120),
        MessagePriority::P3 => Some(150),
        _ => None,
    }
}

/// Malformed-input error — defensive parse never panics (BLE-002 AC-6 pattern).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    PayloadTooLarge(usize),
    Truncated(usize),
    BadVersion(u8),
    BadPriority(u8),
    Oversized(usize),
    BadEscape,
    DanglingEscape,
    NoEnd,
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::PayloadTooLarge(n) => write!(
                f,
                "payload {n} B exceeds {MAX_PAYLOAD_BYTES}-B frame capacity"
            ),
            FrameError::Truncated(n) => write!(f, "frame truncated at {n} B"),
            FrameError::BadVersion(v) => write!(f, "unknown frame version {v}"),
            FrameError::BadPriority(p) => write!(f, "unknown priority byte {p}"),
            FrameError::Oversized(n) => write!(f, "decoded input {n} B over safety cap"),
            FrameError::BadEscape => write!(f, "SLIP: invalid escape sequence"),
            FrameError::DanglingEscape => write!(f, "SLIP: escape at end of input"),
            FrameError::NoEnd => write!(f, "SLIP: missing END terminator"),
        }
    }
}

impl std::error::Error for FrameError {}

/// The shared LoRa frame format carried by BOTH bridge carriers (BLE GATT and
/// USB-SLIP) — one serializer, two transports (D-3).
///
/// Wire layout (≤255 B total): `[ver u8][prio u8][msg_id 16B][payload ≤237]`.
/// The frame length is implied by the carrier (SLIP END run / single ATT
/// write), so no on-air length field spends scarce bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoRaFrame {
    pub priority: MessagePriority,
    pub message_id: MessageId,
    pub payload: Vec<u8>,
}

impl LoRaFrame {
    /// Encode to ≤255 B. Refuses oversized payloads (fragmentation upstream).
    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        if self.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(FrameError::PayloadTooLarge(self.payload.len()));
        }
        let mut buf = Vec::with_capacity(HEADER_LEN + self.payload.len());
        buf.push(FRAME_VERSION);
        buf.push(self.priority.as_u8());
        buf.extend_from_slice(&self.message_id.0);
        buf.extend_from_slice(&self.payload);
        Ok(buf)
    }

    /// Defensive decode of one exact-length carrier frame: truncated /
    /// bad-version / bad-priority / oversized inputs are rejected without
    /// panic or big alloc.
    pub fn decode(bytes: &[u8]) -> Result<LoRaFrame, FrameError> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(FrameError::Oversized(bytes.len()));
        }
        if bytes.len() < HEADER_LEN {
            return Err(FrameError::Truncated(bytes.len()));
        }
        if bytes[0] != FRAME_VERSION {
            return Err(FrameError::BadVersion(bytes[0]));
        }
        let priority =
            MessagePriority::from_u8(bytes[1]).ok_or(FrameError::BadPriority(bytes[1]))?;
        let mut message_id = [0u8; 16];
        message_id.copy_from_slice(&bytes[2..18]);
        Ok(LoRaFrame {
            priority,
            message_id: MessageId(message_id),
            payload: bytes[HEADER_LEN..].to_vec(),
        })
    }
}

const SLIP_END: u8 = 0xC0;
const SLIP_ESC: u8 = 0xDB;
const SLIP_ESC_END: u8 = 0xDC;
const SLIP_ESC_ESC: u8 = 0xDD;
/// Alloc guard for defensive SLIP decode (frames are ≤255 B; allow slack).
const SLIP_DECODE_CAP: usize = 4096;

/// RFC 1055-style SLIP encoding (USB-CDC-ACM carrier): escapes END/ESC and
/// appends one trailing END delimiter.
pub fn encode_slip(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 2);
    for &b in data {
        match b {
            SLIP_END => out.extend_from_slice(&[SLIP_ESC, SLIP_ESC_END]),
            SLIP_ESC => out.extend_from_slice(&[SLIP_ESC, SLIP_ESC_ESC]),
            _ => out.push(b),
        }
    }
    out.push(SLIP_END);
    out
}

/// Decode one SLIP-framed payload. Leading END bytes are tolerated (line
/// noise flush); malformed escapes, dangling ESC, missing END, or decoded
/// sizes beyond the safety cap are errors — never panics, never huge allocs.
pub fn decode_slip(bytes: &[u8]) -> Result<Vec<u8>, FrameError> {
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len().min(SLIP_DECODE_CAP));
    let mut escaped = false;
    for &b in bytes {
        if out.len() > SLIP_DECODE_CAP {
            return Err(FrameError::Oversized(out.len()));
        }
        match b {
            SLIP_END => {
                if !out.is_empty() {
                    return Ok(out);
                }
                // leading delimiter(s): keep scanning
            }
            SLIP_ESC => {
                if escaped {
                    return Err(FrameError::BadEscape);
                }
                escaped = true;
            }
            _ if escaped => {
                out.push(match b {
                    SLIP_ESC_END => SLIP_END,
                    SLIP_ESC_ESC => SLIP_ESC,
                    _ => return Err(FrameError::BadEscape),
                });
                escaped = false;
            }
            _ => out.push(b),
        }
    }
    if escaped {
        return Err(FrameError::DanglingEscape);
    }
    Err(FrameError::NoEnd)
}

// ==== 4. Link adapter seam — AT-serial vs SPI-native (D-1/D-3) ====

/// Module diagnostics aggregated over the link (design §2.4): consumed by
/// LORA_STATUS telemetry and OBS-001 `iris.transport.lora.*`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinkStatus {
    pub rssi_dbm: i16,
    pub snr_db: i8,
    /// Fraction of the hourly duty budget the MODULE believes remains
    /// (informational; the authoritative number lives in [`DutyCycleTracker`]).
    pub duty_remaining_fraction: f32,
    pub battery_pct: u8,
}

/// Link-layer failure. `HardwareGated` marks the BLK-0005 boundary: the real
/// AT/SPI firmware surface lands with hardware procurement + EXP-LORA-001.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoRaLinkError {
    HardwareGated,
    Closed,
    Io(String),
    /// RF-4: the adapter/firmware confirms the PA never keyed up for this
    /// frame (e.g. rejected before transmit). Distinct from `Io`, which
    /// covers faults that CAN follow a real emission (a write that
    /// succeeded but whose ack read timed out, a mid-transmission USB
    /// detach) — only this variant is proof of no airtime spent, which is
    /// what `try_send_inner`'s refund decision actually needs.
    NotTransmitted(String),
}

impl std::fmt::Display for LoRaLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoRaLinkError::HardwareGated => write!(f, "link hardware-gated (BLK-0005/GAP-004)"),
            LoRaLinkError::Closed => write!(f, "link closed"),
            LoRaLinkError::Io(m) => write!(f, "link io: {m}"),
            LoRaLinkError::NotTransmitted(m) => write!(f, "not transmitted: {m}"),
        }
    }
}

impl std::error::Error for LoRaLinkError {}

/// MG-39/MG-41: every call site used to wrap *any* `LoRaLinkError` variant
/// into `TransportError::Io(String)` uniformly — collapsing "hardware
/// gated" (permanent, don't retry), "link closed" (needs a fresh connect),
/// and a genuine I/O failure (may be transient) into one opaque string,
/// defeating the retry-vs-abandon distinction those fixes exist to enable.
/// `context` is prefixed onto the message for the existing "lora link tx/
/// rx/status probe" call-site labels.
fn lora_link_err(context: &str, e: LoRaLinkError) -> TransportError {
    match e {
        LoRaLinkError::HardwareGated => TransportError::HardwareUnavailable,
        LoRaLinkError::Closed => TransportError::NotConnected,
        LoRaLinkError::Io(msg) => TransportError::Io {
            kind: std::io::ErrorKind::Other,
            msg: format!("{context}: {msg}"),
        },
        LoRaLinkError::NotTransmitted(msg) => TransportError::Io {
            kind: std::io::ErrorKind::Other,
            msg: format!("{context}: not transmitted: {msg}"),
        },
    }
}

/// The seam between the IRIS core and an external LoRa module's firmware
/// (`open/close/tx/rx/status`) — abstracts AT-serial (E22 UART) vs SPI-native
/// (SX1262 HAT) so the Rust core never depends on a module-family API
/// (the BleAdapter lesson, DEC-LORA-0005).
///
/// # Implementation contract (RT-103/RT-110, BLK-0005 firmware notes)
/// - After a successful `close()`, `tx()`/`rx()` MUST return
///   [`LoRaLinkError::Closed`] — correctness of the transport's shutdown
///   path relies on it.
/// - `rx()` returns **at most ONE SLIP-framed frame per call**; bridges that
///   read coalesced serial batches must buffer internally and emit one frame
///   per poll. Frames after the first END in a single buffer are NOT
///   extracted by [`decode_slip`].
#[async_trait]
pub trait LoRaLinkAdapter: Send + Sync + 'static {
    async fn open(&self) -> Result<(), LoRaLinkError>;
    async fn close(&self) -> Result<(), LoRaLinkError>;
    /// Transmit one SLIP-framed [`LoRaFrame`].
    async fn tx(&self, frame_slip: &[u8]) -> Result<(), LoRaLinkError>;
    /// Poll one inbound SLIP frame; `None` = nothing received yet.
    async fn rx(&self) -> Result<Option<Vec<u8>>, LoRaLinkError>;
    async fn status(&self) -> Result<LinkStatus, LoRaLinkError>;
    /// RF-1: set the module's PHY configuration. `try_send_inner` calls
    /// this whenever the profile required by the message being sent
    /// (`profile_for_priority`) differs from the last-applied one — the
    /// duty tracker bills airtime computed FOR a profile it never actually
    /// confirmed the module is transmitting AT, unless something makes
    /// the two agree. Infallible-to-ignore-at-your-peril: nothing in this
    /// trait's OTHER methods depends on `configure` ever having been
    /// called, by design (a firmware that silently keeps its own default
    /// SF/power regardless of this call is still memory/type-safe to use
    /// — it is simply non-compliant, which `attach_adapter`'s
    /// `read_config` check below exists to catch at attach time).
    async fn configure(&self, profile: &RadioProfile, max_erp_dbm: i8) -> Result<(), LoRaLinkError>;
    /// RF-1: read back the module's actual, currently-active PHY
    /// configuration. `attach_adapter` calls this once, after `open()`,
    /// and refuses to promote the transport to `Available` if the
    /// reported profile's frequency is outside the compliance band or its
    /// power exceeds the compliance ceiling — a module that will not (or
    /// cannot) confirm compliant config never becomes selectable.
    async fn read_config(&self) -> Result<(RadioProfile, i8), LoRaLinkError>;
}

/// EBYTE E22-900M30S over USB-CDC/UART speaking the vendor AT set —
/// **shape only**: every operation reports [`LoRaLinkError::HardwareGated`]
/// until the BLK-0005 hardware leg lands (EXP-LORA-001, Month 8).
#[derive(Debug, Clone)]
pub struct AtSerialAdapter {
    pub port: String,
    pub baud: u32,
}

impl AtSerialAdapter {
    pub fn new(port: impl Into<String>, baud: u32) -> Self {
        AtSerialAdapter {
            port: port.into(),
            baud,
        }
    }
}

#[async_trait]
impl LoRaLinkAdapter for AtSerialAdapter {
    async fn open(&self) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn close(&self) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn tx(&self, _frame_slip: &[u8]) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn rx(&self) -> Result<Option<Vec<u8>>, LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn status(&self) -> Result<LinkStatus, LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn configure(&self, _profile: &RadioProfile, _max_erp_dbm: i8) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn read_config(&self) -> Result<(RadioProfile, i8), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
}

/// Waveshare SX1262 LoRa HAT over SPI (RPi gateway) — **shape only**, same
/// BLK-0005 hardware gate as [`AtSerialAdapter`].
#[derive(Debug, Clone)]
pub struct SpiNativeAdapter {
    pub device: String,
}

impl SpiNativeAdapter {
    pub fn new(device: impl Into<String>) -> Self {
        SpiNativeAdapter {
            device: device.into(),
        }
    }
}

#[async_trait]
impl LoRaLinkAdapter for SpiNativeAdapter {
    async fn open(&self) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn close(&self) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn tx(&self, _frame_slip: &[u8]) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn rx(&self) -> Result<Option<Vec<u8>>, LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn status(&self) -> Result<LinkStatus, LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn configure(&self, _profile: &RadioProfile, _max_erp_dbm: i8) -> Result<(), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
    async fn read_config(&self) -> Result<(RadioProfile, i8), LoRaLinkError> {
        Err(LoRaLinkError::HardwareGated)
    }
}

/// Deterministic in-memory link for tests/conformance (pattern:
/// `SimulatedBleAdapter`). Two instances joined by [`SimulatedLoRaAdapter::
/// connect_pair`] exchange SLIP frames with configurable packet loss and
/// propagation delay; all randomness is seeded ChaCha8 (SIM-001 precedent).
#[derive(Clone)]
pub struct SimulatedLoRaAdapter(Arc<SimWire>);

struct SimWire {
    opened: std::sync::atomic::AtomicBool,
    loss_rate: f32,
    delay_ms: u64,
    state: Mutex<SimWireState>,
}

struct SimWireState {
    peer: Option<Arc<SimWire>>,
    inbound: VecDeque<PendingFrame>,
    rng: ChaCha8Rng,
    /// RF-1: the module's simulated PHY config, mutated by `configure()`
    /// and read back by `read_config()`. Defaults to the compliant
    /// `DEFAULT_RADIO_PROFILE`/14 dBm so a test that never calls
    /// `configure()` still attaches cleanly — a test that wants to
    /// exercise `attach_adapter`'s rejection path calls `configure()`
    /// with a bad profile/power BEFORE `attach_adapter`, simulating a
    /// module that booted already misconfigured.
    configured: (RadioProfile, i8),
}

struct PendingFrame {
    ready_at: Instant,
    data: Vec<u8>,
}

/// Simulation knobs for [`SimulatedLoRaAdapter`].
#[derive(Debug, Clone)]
pub struct SimLoRaConfig {
    /// Per-TX loss probability in [0, 1] (radio fade proxy).
    pub packet_loss_rate: f32,
    /// Propagation delay applied before the frame becomes receivable.
    pub delay_ms: u64,
    /// Seed for deterministic reproduction.
    pub seed: u64,
}

impl Default for SimLoRaConfig {
    fn default() -> Self {
        SimLoRaConfig {
            packet_loss_rate: 0.0,
            delay_ms: 0,
            seed: 42,
        }
    }
}

impl SimulatedLoRaAdapter {
    pub fn new(cfg: SimLoRaConfig) -> Self {
        SimulatedLoRaAdapter(Arc::new(SimWire {
            opened: std::sync::atomic::AtomicBool::new(false),
            loss_rate: cfg.packet_loss_rate.clamp(0.0, 1.0),
            delay_ms: cfg.delay_ms,
            state: Mutex::new(SimWireState {
                peer: None,
                inbound: VecDeque::new(),
                rng: ChaCha8Rng::seed_from_u64(cfg.seed),
                configured: (DEFAULT_RADIO_PROFILE, 14),
            }),
        }))
    }

    /// Wire two adapters to each other (both directions).
    pub fn connect_pair(a: &SimulatedLoRaAdapter, b: &SimulatedLoRaAdapter) {
        a.0.state.lock().unwrap_or_else(|p| p.into_inner()).peer = Some(Arc::clone(&b.0));
        b.0.state.lock().unwrap_or_else(|p| p.into_inner()).peer = Some(Arc::clone(&a.0));
    }
}

#[async_trait]
impl LoRaLinkAdapter for SimulatedLoRaAdapter {
    async fn open(&self) -> Result<(), LoRaLinkError> {
        self.0.opened.store(true, Ordering::Release);
        Ok(())
    }

    async fn close(&self) -> Result<(), LoRaLinkError> {
        self.0.opened.store(false, Ordering::Release);
        Ok(())
    }

    async fn tx(&self, frame_slip: &[u8]) -> Result<(), LoRaLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(LoRaLinkError::Closed);
        }
        let peer = {
            let mut st = self.0.state.lock().unwrap_or_else(|p| p.into_inner());
            let lost = rand::Rng::gen_range(&mut st.rng, 0.0..1.0) < self.0.loss_rate;
            if lost {
                return Ok(()); // transmitted into a fade — silently gone
            }
            st.peer.clone()
        };
        if let Some(peer) = peer {
            let pending = PendingFrame {
                ready_at: Instant::now() + Duration::from_millis(self.0.delay_ms),
                data: frame_slip.to_vec(),
            };
            peer.state.lock().unwrap_or_else(|p| p.into_inner()).inbound.push_back(pending);
        }
        // An unpaired open link still transmits (raw radio broadcasts to nobody).
        Ok(())
    }

    async fn rx(&self) -> Result<Option<Vec<u8>>, LoRaLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(LoRaLinkError::Closed);
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

    async fn status(&self) -> Result<LinkStatus, LoRaLinkError> {
        // Fixed sim telemetry (SIMULATION_VALIDATED only).
        Ok(LinkStatus {
            rssi_dbm: -85,
            snr_db: 9,
            duty_remaining_fraction: 1.0,
            battery_pct: 100,
        })
    }

    async fn configure(&self, profile: &RadioProfile, max_erp_dbm: i8) -> Result<(), LoRaLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(LoRaLinkError::Closed);
        }
        self.0.state.lock().unwrap_or_else(|p| p.into_inner()).configured = (*profile, max_erp_dbm);
        Ok(())
    }

    async fn read_config(&self) -> Result<(RadioProfile, i8), LoRaLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(LoRaLinkError::Closed);
        }
        Ok(self.0.state.lock().unwrap_or_else(|p| p.into_inner()).configured)
    }
}

// ==== 5. Simulated link budget (RES-0027 RQ-6 / D-6, DEC-LORA-0007) ====

/// Deterministic link-budget model — **SIMULATION_VALIDATED only** (never a
/// field claim): AN1200.13 receive-sensitivity anchors + Okumura-Hata urban
/// path loss. EXP-LORA-001 measured traces replace these parameters later
/// (calibration hooks = swap the constants behind these pure functions).
pub struct SimLinkBudget;

impl SimLinkBudget {
    /// Okumura-Hata model geometry defaults.
    pub const BASE_STATION_M: f64 = 30.0;
    pub const MOBILE_M: f64 = 1.5;
    /// Model validity window (km) — outputs are clamped to it.
    pub const MIN_RANGE_KM: f64 = 0.01;
    pub const MAX_RANGE_KM: f64 = 20.0;

    /// Receiver sensitivity (dBm) at BW125 per Semtech AN1200.13.
    /// SF9 = −129 and SF12 = −136 are the design-pinned anchors.
    pub fn sensitivity_dbm(sf: u8) -> Option<f32> {
        match sf {
            7 => Some(-123.0),
            8 => Some(-126.0),
            9 => Some(-129.0),
            10 => Some(-132.0),
            11 => Some(-133.0),
            12 => Some(-136.0),
            _ => None,
        }
    }

    /// Okumura-Hata urban path loss (dB), medium-city correction, for
    /// 150–1500 MHz carriers (866 qualifies).
    /// RF-15: returns `None` when `dist_km` is outside the model's validity
    /// window `[MIN_RANGE_KM, MAX_RANGE_KM]` — callers must handle the
    /// out-of-range case rather than silently receiving the clamped 20 km value.
    pub fn okumura_hata_urban_loss_db(freq_mhz: f64, dist_km: f64) -> Option<f64> {
        if !(Self::MIN_RANGE_KM..=Self::MAX_RANGE_KM).contains(&dist_km) {
            return None;
        }
        let lf = freq_mhz.log10();
        let lhb = Self::BASE_STATION_M.log10();
        let a_hm = (1.1 * lf - 0.7) * Self::MOBILE_M - (1.56 * lf - 0.8);
        let c1 = 69.55 + 26.16 * lf - 13.82 * lhb - a_hm;
        let slope = 44.9 - 6.55 * lhb;
        Some(c1 + slope * dist_km.log10())
    }

    /// Max range (km) where TX power still closes the link for `profile`
    /// (margin = tx − sensitivity; solve `L(d) = margin`; clamp to validity).
    pub fn max_range_km(profile: &RadioProfile, tx_dbm: i8) -> f64 {
        let sens = f64::from(Self::sensitivity_dbm(profile.sf).unwrap_or(-129.0));
        let margin = f64::from(tx_dbm) - sens;
        let freq_mhz = profile.freq_hz as f64 / 1.0e6;
        let lf = freq_mhz.log10();
        let lhb = Self::BASE_STATION_M.log10();
        let a_hm = (1.1 * lf - 0.7) * Self::MOBILE_M - (1.56 * lf - 0.8);
        let c1 = 69.55 + 26.16 * lf - 13.82 * lhb - a_hm;
        let slope = 44.9 - 6.55 * lhb;
        let log_d = (margin - c1) / slope;
        10f64
            .powf(log_d)
            .clamp(Self::MIN_RANGE_KM, Self::MAX_RANGE_KM)
    }

    /// Delivery probability model: unity while the link margin holds, then a
    /// linear fade to a 0.05 floor across −9 dB of shadowing allowance.
    /// RF-15: returns `0.0` when `dist_km` is outside the model's validity
    /// window — unlike the previous clamp, this signals "cannot close the link"
    /// rather than substituting the 20 km answer for a 500 km question.
    /// Deterministic pure function (SIMULATION_VALIDATED only).
    pub fn delivery_probability(profile: &RadioProfile, tx_dbm: i8, dist_km: f64) -> f32 {
        let sens = Self::sensitivity_dbm(profile.sf).unwrap_or(-129.0);
        let freq_mhz = profile.freq_hz as f64 / 1.0e6;
        // RF-15: None outside model validity → treat as unreachable (0.0).
        let loss = match Self::okumura_hata_urban_loss_db(freq_mhz, dist_km) {
            Some(l) => l,
            None => return 0.0,
        };
        let margin_db = f64::from(tx_dbm) - f64::from(sens) - loss;
        if margin_db >= 0.0 {
            1.0
        } else {
            (1.0 + (margin_db / 9.0) as f32).clamp(0.05, 1.0)
        }
    }
}

// ==== 6. OBS-001 metrics seam (AC-10) ====

/// Counters surfaced via `iris.transport.lora.*` (payload bytes never logged).
#[derive(Debug, Default)]
pub struct LoRaMetrics {
    packets_tx: AtomicU64,
    packets_rx: AtomicU64,
    bytes_tx: AtomicU64,
    bytes_rx: AtomicU64,
    airtime_consumed_ms: AtomicU64,
    duty_refusals: AtomicU64,
    /// RT-104: sends whose duty reservation was refunded after a TX failure
    /// PROVEN not to have reached the air (RF-4: `Closed`/`NotTransmitted`).
    tx_failures: AtomicU64,
    /// RF-4: sends whose duty reservation was KEPT after a TX failure that
    /// does not prove no emission (`Io`/`HardwareGated`) — the reservation
    /// stays billed (fail-closed on the legal budget) even though the
    /// caller never got a receipt, so telemetry can still distinguish
    /// "burned airtime, outcome unknown" from a clean refund.
    tx_unknown_outcome: AtomicU64,
    /// RT-105: inbound frames dropped for malformed SLIP/frame encoding.
    malformed_rx: AtomicU64,
    /// RF-17: adapter.close() calls that returned an error on teardown.
    close_failures: AtomicU64,
    /// RF-12: backlog entries dropped because the error is permanent
    /// (Protocol / PolicyDenied / MessageTooLarge — will never succeed).
    backlog_protocol_drops: AtomicU64,
}

/// Point-in-time copy of [`LoRaMetrics`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoRaMetricsSnapshot {
    pub packets_tx: u64,
    pub packets_rx: u64,
    pub bytes_tx: u64,
    pub bytes_rx: u64,
    pub airtime_consumed_ms: u64,
    pub duty_refusals: u64,
    pub tx_failures: u64,
    /// RF-4: TX failures where the reservation was kept (unproven no-emission).
    pub tx_unknown_outcome: u64,
    pub malformed_rx: u64,
    /// RF-17: adapter close errors on detach or shutdown.
    pub close_failures: u64,
    /// RF-12: backlog entries permanently dropped (Protocol/PolicyDenied/MessageTooLarge).
    pub backlog_protocol_drops: u64,
}

impl LoRaMetrics {
    fn record_tx(&self, frame_bytes: usize, airtime: u64) {
        self.packets_tx.fetch_add(1, Ordering::Relaxed);
        self.bytes_tx
            .fetch_add(frame_bytes as u64, Ordering::Relaxed);
        self.airtime_consumed_ms
            .fetch_add(airtime, Ordering::Relaxed);
    }

    fn record_rx(&self, frame_bytes: usize) {
        self.packets_rx.fetch_add(1, Ordering::Relaxed);
        self.bytes_rx
            .fetch_add(frame_bytes as u64, Ordering::Relaxed);
    }

    fn record_refusal(&self) {
        self.duty_refusals.fetch_add(1, Ordering::Relaxed);
    }

    fn record_tx_failure(&self) {
        self.tx_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn record_tx_unknown_outcome(&self) {
        self.tx_unknown_outcome.fetch_add(1, Ordering::Relaxed);
    }

    fn record_malformed_rx(&self) {
        self.malformed_rx.fetch_add(1, Ordering::Relaxed);
    }

    fn record_close_failure(&self) {
        self.close_failures.fetch_add(1, Ordering::Relaxed);
    }

    fn record_backlog_protocol_drop(&self) {
        self.backlog_protocol_drops.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> LoRaMetricsSnapshot {
        LoRaMetricsSnapshot {
            packets_tx: self.packets_tx.load(Ordering::Relaxed),
            packets_rx: self.packets_rx.load(Ordering::Relaxed),
            bytes_tx: self.bytes_tx.load(Ordering::Relaxed),
            bytes_rx: self.bytes_rx.load(Ordering::Relaxed),
            airtime_consumed_ms: self.airtime_consumed_ms.load(Ordering::Relaxed),
            duty_refusals: self.duty_refusals.load(Ordering::Relaxed),
            tx_failures: self.tx_failures.load(Ordering::Relaxed),
            tx_unknown_outcome: self.tx_unknown_outcome.load(Ordering::Relaxed),
            malformed_rx: self.malformed_rx.load(Ordering::Relaxed),
            close_failures: self.close_failures.load(Ordering::Relaxed),
            backlog_protocol_drops: self.backlog_protocol_drops.load(Ordering::Relaxed),
        }
    }
}

// ==== 7. Backlog queue (REQ-ROUTE-C-003 / D-5, DEC-LORA-0003) ====

/// RF-13: per-class backlog TTL. P0/SOS is kept short — it should trigger a
/// P0-switch, not sit queued for an hour. Higher classes get proportionally
/// longer windows since they tolerate delay.
fn backlog_ttl(priority: MessagePriority) -> Duration {
    match priority {
        MessagePriority::P0 => Duration::from_secs(300),    // 5 min
        MessagePriority::P1 => Duration::from_secs(1_800),  // 30 min
        MessagePriority::P2 => Duration::from_secs(3_600),  // 60 min
        MessagePriority::P3 => Duration::from_secs(14_400), // 4 h
        _                   => Duration::from_secs(43_200), // 12 h (P4+)
    }
}

/// One held send: its priority plus the payload waiting for budget recovery.
/// RF-13: `enqueued_at` drives per-class TTL sweeps in push/pop.
/// RF-23: `next_attempt_at` gates retry timing — `drain_backlog` skips an
/// entry until the wall clock passes this instant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogEntry<T> {
    pub priority: MessagePriority,
    pub enqueued_at: Instant,
    /// RF-23: earliest time this entry may be retried. Initialized to the
    /// enqueue instant so entries are immediately eligible on first drain.
    pub next_attempt_at: Instant,
    pub item: T,
}

/// Priority-ordered holding queue: entries drain highest-priority-first as
/// duty budget recovers (recovery ≈1% per 100 s of silence — routing-layer
/// property). FIFO within the same priority class.
#[derive(Debug)]
pub struct BacklogQueue<T> {
    entries: Mutex<Vec<BacklogEntry<T>>>,
}

impl<T> Default for BacklogQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BacklogQueue<T> {
    pub fn new() -> Self {
        BacklogQueue {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Insert keeping ascending-priority order (P0 first), FIFO within class.
    /// RF-13: (a) sweeps expired entries before checking capacity; (b) on cap,
    /// evicts the lowest-priority entry if the incoming has strictly higher
    /// priority (priority inversion guard). Returns false only when at capacity
    /// AND the lowest entry is not lower-priority than the incoming one.
    pub fn push(&self, priority: MessagePriority, item: T) -> bool {
        let mut g = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        // RF-13a: sweep stale entries so a full queue of expired traffic
        // cannot block a fresh P0 enqueue.
        g.retain(|e| now.duration_since(e.enqueued_at) < backlog_ttl(e.priority));
        if g.len() >= MAX_BACKLOG_ENTRIES {
            // RF-13b: evict the lowest-priority entry when the incoming is
            // strictly more urgent — prevents priority inversion at the cap.
            match g.last() {
                Some(lowest) if lowest.priority > priority => { g.pop(); }
                _ => return false,
            }
        }
        let pos = g.partition_point(|e| e.priority <= priority);
        g.insert(pos, BacklogEntry { priority, enqueued_at: now, next_attempt_at: now, item });
        true
    }

    /// SAT-RT-104 lesson (mirrored from the satellite transport review):
    /// re-insert an item that was ALREADY queued (drain deferral) — ignores
    /// the depth cap so concurrent enqueues during a drain can never silently
    /// drop held traffic. May temporarily exceed [`MAX_BACKLOG_ENTRIES`].
    /// Preserves the caller-set `next_attempt_at` so retry scheduling survives
    /// the deferred re-insert (RF-23).
    pub fn push_deferred(&self, entry: BacklogEntry<T>) {
        let mut g = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let pos = g.partition_point(|e| e.priority <= entry.priority);
        g.insert(pos, entry);
    }

    /// Pop the highest-priority non-expired entry (front of the sorted vec).
    /// RF-13: sweeps all expired entries before returning so callers never
    /// receive a stale payload.
    pub fn pop_highest(&self) -> Option<BacklogEntry<T>> {
        let mut g = self.entries.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        g.retain(|e| now.duration_since(e.enqueued_at) < backlog_ttl(e.priority));
        if g.is_empty() {
            None
        } else {
            Some(g.remove(0))
        }
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|p| p.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ==== 8. LoRaTransport — the Transport impl wiring it together ====

fn lora_capabilities() -> TransportCapabilities {
    TransportCapabilities {
        // Envelope capacity per frame: 255-B PHY cap minus the 18-B IRIS
        // header — exactly the 237-B P0 envelope budget (RES-0003).
        max_message_size: MAX_PAYLOAD_BYTES,
        supports_broadcast: true,
        supports_unicast: true,
        supports_multicast: false,
        range_m_min: 2000,
        range_m_max: 15000,
        range_m_typical: 5000,
        typical_throughput_bps: raw_rate_bps(&DEFAULT_RADIO_PROFILE),
        typical_latency_ms: 1500,
        requires_infrastructure: false,
        supports_background_android: false,
        supports_background_ios: false,
        requires_special_hardware: true,
        cost_class: TransportCostClass::Free,
        regulatory_band: Some(String::from(
            "WPC 865-868 MHz SRD (G.S.R. 853(E) 2021), <=25 mW e.r.p., <=1% duty",
        )),
        conflict_group: crate::transport::RadioConflictGroup::SubGHz,
    }
}

/// LoRa gateway transport: wires the [`DutyCycleTracker`] (hard legal floor),
/// the [`LoRaLinkAdapter`] dongle seam (hot-plug register/deregister), the
/// priority [`BacklogQueue`], and OBS-001 counters onto the core `Transport`
/// trait. Platform-neutral — the phone/RPi hosts the module over BLE GATT or
/// USB-SLIP; the radio itself is external (BLK-0005-gated firmware).
pub struct LoRaTransport {
    id: TransportId,
    display: String,
    caps: TransportCapabilities,
    state: AtomicState,
    state_tx: broadcast::Sender<TransportStateEvent>,
    incoming_tx: broadcast::Sender<IncomingMessage>,
    tracker: Arc<DutyCycleTracker>,
    adapter: tokio::sync::RwLock<Option<Arc<dyn LoRaLinkAdapter>>>,
    backlog: BacklogQueue<SerializedMessage>,
    metrics: LoRaMetrics,
    last_priority: AtomicU8,
    shutdown_flag: AtomicBool,
    /// RF-1: the profile `configure()` was last (successfully) called
    /// with, so `try_send_inner` only re-configures the module when the
    /// send actually needs a different profile than what's already
    /// active — avoiding a `configure` round trip on every single send.
    last_configured_profile: Mutex<Option<RadioProfile>>,
    /// RF-11: consecutive tx() failures; resets on success. Demotes to
    /// Degraded at threshold so the manager stops ranking a dead link first.
    consecutive_tx_failures: AtomicU32,
}

// SYS-2: deadline constants for LoRa adapter calls.
const OPEN_TIMEOUT: Duration = Duration::from_secs(10);
const TX_TIMEOUT: Duration = Duration::from_secs(30);

impl LoRaTransport {
    pub fn new(id: &str) -> Self {
        Self::with_tracker(id, Arc::new(DutyCycleTracker::new()))
    }

    pub fn with_tracker(id: &str, tracker: Arc<DutyCycleTracker>) -> Self {
        let (state_tx, _) = broadcast::channel(64);
        let (incoming_tx, _) = broadcast::channel(1024);
        LoRaTransport {
            id: TransportId::from(id),
            display: String::from("LoRa Gateway"),
            caps: lora_capabilities(),
            state: AtomicState::default(),
            state_tx,
            incoming_tx,
            tracker,
            adapter: tokio::sync::RwLock::new(None),
            backlog: BacklogQueue::new(),
            metrics: LoRaMetrics::default(),
            last_priority: AtomicU8::new(MessagePriority::P4.as_u8()),
            shutdown_flag: AtomicBool::new(false),
            last_configured_profile: Mutex::new(None),
            consecutive_tx_failures: AtomicU32::new(0),
        }
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        self.state_tx.send(TransportStateEvent {
            transport_id: self.id.clone(),
            new_state: state,
        });
    }

    /// Dongle plug: attach the module's link adapter, bring the link up, and
    /// promote to Available. RT-103: an `open()` failure is NOT ignored —
    /// a dead dongle must not be stored behind an Available state (the
    /// manager would select it and every send would fail).
    pub async fn attach_adapter(
        &self,
        adapter: Arc<dyn LoRaLinkAdapter>,
    ) -> Result<(), LoRaLinkError> {
        if self.shutdown_flag.load(Ordering::Acquire) {
            return Err(LoRaLinkError::Closed);
        }
        tokio::time::timeout(OPEN_TIMEOUT, adapter.open())
            .await
            .map_err(|_| LoRaLinkError::Io("open timed out".to_string()))??;
        // RF-1: a module that will not (or cannot) confirm compliant PHY
        // config never becomes selectable — without this, the duty
        // tracker bills airtime for a profile it never actually verified
        // the module is transmitting at, making its own compliance
        // evidence unfalsifiable (this finding's core concern). Checks
        // only frequency + power (not SF/BW/coding rate) — `try_send_
        // inner` reconfigures per-send for the priority-specific profile,
        // so attach time only needs to confirm the module's regulatory
        // ceiling agrees with this tracker's, not any one specific SF.
        let (reported_profile, reported_power_dbm) =
            tokio::time::timeout(OPEN_TIMEOUT, adapter.read_config())
                .await
                .map_err(|_| LoRaLinkError::Io("read_config timed out".to_string()))??;
        let cfg = self.tracker.compliance_config();
        if reported_profile.freq_hz != cfg.freq_hz {
            // Cross-checks RadioProfile.freq_hz against ComplianceConfig.
            // freq_hz — DEFAULT_RADIO_PROFILE/P0_RADIO_PROFILE and
            // ComplianceConfig all pin IN865_1 (865_062_500 Hz); this
            // assertion catches any drift (RF-1 evidence section).
            return Err(LoRaLinkError::Io(format!(
                "module reports {} Hz, compliance config pins {} Hz",
                reported_profile.freq_hz, cfg.freq_hz
            )));
        }
        if reported_power_dbm > cfg.max_erp_dbm {
            return Err(LoRaLinkError::Io(format!(
                "module reports {reported_power_dbm} dBm e.r.p., exceeds the {} dBm Table-I ceiling",
                cfg.max_erp_dbm
            )));
        }
        *self.adapter.write().await = Some(adapter);
        // A freshly-attached module's actual configured profile is
        // whatever it reported above, not necessarily what the first send
        // will need -- reset so try_send_inner's first send always
        // reconfigures rather than trusting a stale assumption from a
        // previous adapter instance.
        *self.last_configured_profile.lock().unwrap_or_else(|p| p.into_inner()) = None;
        if self.state.load() < TransportState::Available {
            self.set_state(TransportState::Available);
        }
        Ok(())
    }

    /// Dongle unplug: close + drop the adapter; the transport goes
    /// Unavailable (`TransportManager::deregister` removes it from selection).
    pub async fn detach_adapter(&self) {
        if let Some(adapter) = self.adapter.write().await.take() {
            if let Err(e) = adapter.close().await {
                tracing::warn!(
                    target: "iris.transport.lora",
                    error = %e,
                    "adapter close failed on detach; OS handle may be leaked"
                );
                self.metrics.record_close_failure();
            }
        }
        if self.state.load() >= TransportState::Available {
            self.set_state(TransportState::Unavailable);
        }
    }

    /// R7 seam: fraction of the hourly duty budget remaining. The routing
    /// engine watches this to P0-switch to a full-budget transport BEFORE a
    /// refusal ever happens (P0 is never held; DEC-LORA-0003).
    pub fn duty_cycle_remaining(&self) -> f32 {
        self.tracker.duty_cycle_remaining_fraction()
    }

    pub fn metrics_snapshot(&self) -> LoRaMetricsSnapshot {
        self.metrics.snapshot()
    }

    /// Last priority seen via `set_send_priority_hint` (selection hint).
    pub fn send_priority_hint(&self) -> MessagePriority {
        MessagePriority::from_u8(self.last_priority.load(Ordering::Relaxed))
            .unwrap_or(MessagePriority::P4)
    }

    /// Hold a send for later drain (REQ-ROUTE-C-003 router seam).
    /// Hold a send for later drain (REQ-ROUTE-C-003 router seam). RT-106:
    /// rejects payloads the transport could never carry and bounds queue
    /// depth (`MAX_BACKLOG_ENTRIES`) against memory exhaustion on
    /// constrained devices.
    pub fn enqueue_backlog(&self, msg: SerializedMessage) -> Result<(), TransportError> {
        // RF-10: P4+ is spec-excluded from LoRa outright (permanent for
        // this transport — MG-40's PolicyDenied, not a size/framing issue
        // MessageTooLarge would wrongly imply is fixable by fragmenting).
        let Some(cap) = max_payload_for(msg.priority) else {
            return Err(TransportError::PolicyDenied("LoRa carries P0-P3 only"));
        };
        if msg.payload.len() > cap {
            // MG-40: resolvable by fragmenting upstream and retrying on
            // this same transport — not a framing/decode failure.
            return Err(TransportError::MessageTooLarge {
                limit: cap,
                actual: msg.payload.len(),
            });
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
        // RF-10: same priority-eligibility + per-class cap as
        // enqueue_backlog — a message could reach here directly via
        // Transport::send() without ever passing through the backlog.
        let Some(cap) = max_payload_for(msg.priority) else {
            return Err(TransportError::PolicyDenied("LoRa carries P0-P3 only"));
        };
        if msg.payload.len() > cap {
            return Err(TransportError::MessageTooLarge {
                limit: cap,
                actual: msg.payload.len(),
            });
        }
        let frame = LoRaFrame {
            priority: msg.priority,
            message_id: msg.message_id,
            payload: msg.payload.clone(),
        };
        let encoded = frame
            .encode()
            .map_err(|e| TransportError::Protocol(e.to_string()))?;
        // RT-101 (SECURITY_REVIEW iter ~165): bill the WHOLE on-air frame —
        // the 18-B IRIS header is transmitted too. Billing payload-only
        // under-counted small frames by up to ~79% and made a Table-I duty
        // exceedance arithmetically reachable. SLIP escapes are host<->module
        // carrier framing, not on-air bytes, and are deliberately not billed.
        let profile = profile_for_priority(msg.priority);
        let airtime = airtime_ms(&profile, encoded.len());
        // RF-7: per-transmission timing (max on-time / min off-time) is a
        // separate constraint from the aggregate window budget below —
        // checked first, since reserving airtime for a transmission that
        // can never legally happen as shaped (or must wait regardless of
        // budget) would just need refunding.
        if let Err(tr) = self.tracker.check_transmission_timing(airtime) {
            tracing::debug!(
                target: "iris.transport.lora",
                timing_refusal = ?tr.reason,
                retry_in_ms = tr.delay_ms,
                "send refused by per-transmission timing constraint"
            );
            self.metrics.record_refusal();
            return Err(match tr.reason {
                // No wait fixes an inherently-too-long single transmission
                // — the caller must fragment smaller and retry. Closest
                // existing variant: permanent for this exact request, but
                // (unlike a true PolicyDenied) retrying on LoRa WOULD work
                // once the message is re-fragmented — there is no clean
                // existing TransportError variant for a *time*-shaped
                // per-request ceiling (MessageTooLarge's limit/actual are
                // byte counts).
                TimingRefusalReason::MaxOnTimeExceeded => TransportError::PolicyDenied(
                    "single LoRa transmission exceeds the regulatory max on-time; message must be fragmented smaller",
                ),
                TimingRefusalReason::MinOffTimeNotElapsed => TransportError::RateLimited {
                    retry_after_ms: tr.delay_ms,
                    remaining_fraction: self.tracker.duty_cycle_remaining_fraction(),
                },
            });
        }
        let admission = match self.tracker.check_and_consume(msg.priority, airtime) {
            Ok(w) => w,
            Err(be) => {
                tracing::debug!(
                    target: "iris.transport.lora",
                    duty_refusal = true,
                    retry_in_ms = be.delay_ms,
                    "send refused by 1% duty budget; re-route or P0-switch"
                );
                self.metrics.record_refusal();
                // RF-5: `NextWindow`/`BudgetExhausted` already compute the
                // next legal send time (RES-0008 R7) — `TransportError::Busy`
                // (pool saturation, no known recovery time) discarded it, so
                // a caller could not tell a duty refusal from a busy socket
                // pool and got no retry time to schedule around.
                return Err(TransportError::RateLimited {
                    retry_after_ms: be.delay_ms,
                    remaining_fraction: be.remaining_fraction,
                });
            }
        };
        // RF-1: (re)configure the module when this send needs a different
        // profile than what's currently active, so the airtime just
        // reserved above is actually billed for the profile the module is
        // really transmitting at. Done after the budget/timing checks
        // (not before) to avoid a wasted hardware round trip on a request
        // that was going to be refused anyway; done before `tx()` so a
        // configure failure can still refund cleanly.
        let needs_configure = *self
            .last_configured_profile
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            != Some(profile);
        if needs_configure {
            let cfg = self.tracker.compliance_config();
            if let Err(e) = adapter.configure(&profile, cfg.max_erp_dbm).await {
                // The reservation was never used -- refund it, same as any
                // other proven-no-emission failure (RF-4).
                self.tracker.refund(admission.token);
                self.metrics.record_tx_failure();
                tracing::debug!(target: "iris.transport.lora", "configure failed before transmit; budget refunded");
                return Err(lora_link_err("lora link configure", e));
            }
            *self
                .last_configured_profile
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = Some(profile);
        }
        let slip = encode_slip(&encoded);
        let tx_result = tokio::time::timeout(TX_TIMEOUT, adapter.tx(&slip))
            .await
            .map_err(|_| LoRaLinkError::Io("tx timed out".to_string()));
        if let Err(e) = tx_result.and_then(|r| r) {
            // RF-4: refund only when the error PROVES no emission happened.
            // `Closed`/`NotTransmitted` are that proof; `Io`/`HardwareGated`
            // can both follow a real emission (a write that succeeded but
            // whose ack read timed out, a mid-transmission USB detach) —
            // regulatory airtime is consumed by the radio, not by the
            // return value, so those keep the reservation (fail-closed on
            // the legal budget) rather than refunding on an assumption the
            // adapter contract never actually promised.
            match &e {
                LoRaLinkError::Closed | LoRaLinkError::NotTransmitted(_) => {
                    // RT-104: the reservation never reached the air — refund
                    // it so a flapping dongle cannot incinerate the budget.
                    self.tracker.refund(admission.token);
                    self.metrics.record_tx_failure();
                    tracing::debug!(target: "iris.transport.lora", "tx failed before transmit; budget refunded");
                }
                LoRaLinkError::Io(_) | LoRaLinkError::HardwareGated => {
                    self.metrics.record_tx_unknown_outcome();
                    tracing::debug!(target: "iris.transport.lora", "tx failed with unproven outcome; budget kept (fail-closed)");
                }
            }
            // RF-11: count consecutive failures; demote to Degraded after 3.
            let n = self.consecutive_tx_failures.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= 3 && self.state.load() == TransportState::Connected {
                tracing::warn!(
                    target: "iris.transport.lora",
                    failures = n,
                    "demoting to Degraded after consecutive tx failures"
                );
                self.set_state(TransportState::Degraded);
            }
            return Err(lora_link_err("lora link tx", e));
        }
        // RF-11: successful tx — reset failure streak and recover to Connected.
        let prev = self.consecutive_tx_failures.swap(0, Ordering::Relaxed);
        if prev > 0 && self.state.load() == TransportState::Degraded {
            self.set_state(TransportState::Connected);
        }
        self.metrics.record_tx(encoded.len(), airtime);
        tracing::debug!(
            target: "iris.transport.lora",
            airtime_ms = airtime,
            profile_sf = profile.sf,
            "lora tx admitted"
        );
        Ok(SendReceipt {
            peer_id: *peer,
            bytes_sent: encoded.len(),
            sent_at: Instant::now(),
        })
    }

    /// Drain up to `max` backlog entries in priority order, sending what the
    /// recovered budget now allows; entries the budget still cannot fit stay
    /// queued (FIFO within their class). The walk continues past refusals:
    /// a lower bucket with free share may still send while an exhausted one
    /// defers (bucket-proportional fairness, DEC-LORA-0003).
    pub async fn drain_backlog(&self, max: usize) -> Vec<(SerializedMessage, SendReceipt)> {
        let mut drained = Vec::new();
        let mut deferred = Vec::new();
        let now = Instant::now();
        'drain: for _ in 0..max {
            let Some(entry) = self.backlog.pop_highest() else {
                break;
            };
            // RF-23: skip entries whose retry window has not yet elapsed.
            if now < entry.next_attempt_at {
                deferred.push(entry);
                continue;
            }
            // Backlog entries carry no destination (raw-LoRa is broadcast at
            // the PHY): receipts use the zero-PeerId fallback convention.
            match self.try_send_inner(&PeerId([0u8; 32]), &entry.item).await {
                Ok(receipt) => drained.push((entry.item, receipt)),
                // RF-12: terminal transport errors — put this entry back and
                // stop draining; the transport cannot deliver anything now.
                Err(TransportError::ShuttingDown | TransportError::NotConnected) => {
                    self.backlog.push_deferred(entry);
                    break 'drain;
                }
                // RF-12: permanent per-message errors — the entry will never
                // succeed on this transport; drop and count.
                Err(
                    TransportError::Protocol(_)
                    | TransportError::PolicyDenied(_)
                    | TransportError::MessageTooLarge { .. },
                ) => {
                    self.metrics.record_backlog_protocol_drop();
                    tracing::warn!(
                        target: "iris.transport.lora",
                        priority = ?entry.priority,
                        "backlog entry permanently dropped (protocol/policy error)"
                    );
                }
                // Transient errors (Busy, RateLimited, Io, …) — defer.
                Err(_) => deferred.push(entry),
            }
        }
        for entry in deferred {
            // SAT-RT-104: deferred items were already queued once — re-insert
            // over-capacity so concurrent enqueues can never drop them.
            self.backlog.push_deferred(entry);
        }
        drained
    }

    /// Poll the link for inbound frames and forward them onto the incoming
    /// stream. Returns how many frames were delivered this call. Sender
    /// attribution is the zero PeerId fallback (WAW-RT-007 precedent) — the
    /// envelope layer performs real verification above this transport.
    pub async fn poll_inbound(&self) -> Result<usize, TransportError> {
        if self.shutdown_flag.load(Ordering::Acquire) {
            return Err(TransportError::ShuttingDown);
        }
        let adapter = self.adapter.read().await.clone();
        let Some(adapter) = adapter else {
            return Err(TransportError::NotConnected);
        };
        let mut delivered = 0usize;
        // RF-9: bound per-call work. An adversarial emitter on the open LoRa
        // medium can keep rx() returning Some forever — cap the loop so the
        // poller task can yield and re-schedule, preventing a livelock.
        for _ in 0..MAX_FRAMES_PER_POLL {
            match adapter.rx().await {
                Ok(Some(slip)) => {
                    // RT-105: a malformed frame must not abort the poll
                    // batch — LoRa is an open medium and garbage from one
                    // node would otherwise deny inbound delivery for every
                    // legitimate frame queued behind it. Skip + count.
                    let decoded =
                        decode_slip(&slip).and_then(|bytes| match LoRaFrame::decode(&bytes) {
                            Ok(frame) => Ok((bytes, frame)),
                            Err(e) => Err(e),
                        });
                    match decoded {
                        Ok((bytes, frame)) => {
                            self.metrics.record_rx(bytes.len());
                            self.incoming_tx.send(IncomingMessage {
                                peer_id: PeerId([0u8; 32]),
                                transport_id: self.id.as_str().to_string(),
                                payload: frame.payload,
                                received_at: Instant::now(),
                            }).ok();
                            delivered += 1;
                        }
                        Err(e) => {
                            self.metrics.record_malformed_rx();
                            tracing::debug!(
                                target: "iris.transport.lora",
                                error = %e,
                                "malformed inbound frame dropped (batch continues)"
                            );
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(lora_link_err("lora link rx", e)),
            }
        }
        Ok(delivered)
    }
}

#[async_trait]
impl Transport for LoRaTransport {
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
        crate::transport::broadcast_stream(self.state_tx.subscribe(), "lora.state")
    }

    async fn discover_peers(
        &self,
        _config: DiscoveryConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = PeerInfo> + Send>>, TransportError> {
        // Gateway-mediated neighbor discovery arrives with the BLK-0005
        // bridge firmware; the mesh discovers peers over BLE/Wi-Fi legs.
        Ok(Box::pin(futures_util::stream::empty()))
    }

    async fn stop_discovery(&self) -> Result<(), TransportError> {
        Ok(())
    }

    async fn start_advertising(&self, _info: NodeAdvertisement) -> Result<(), TransportError> {
        // The LoRa BRIDGE advertises the IRIS service (design §2.4); the
        // phone/RPi app side has no advertising action of its own.
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
        // RT-103: do not promote to Connected on a dead link — probe the
        // module first. A HardwareGated/faulted dongle fails here instead of
        // poisoning selection with a Connected-but-unusable transport.
        adapter
            .status()
            .await
            .map_err(|e| lora_link_err("lora link status probe", e))?;
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
        // RT-108 (recorded contract): raw-LoRa PHY is broadcast — the receipt
        // means "handed to the radio", NEVER "delivered to peer". Downstream
        // ACK/dedup layers are the only authoritative delivery evidence.
        self.try_send_inner(peer, message).await
    }

    fn incoming_messages(&self) -> Pin<Box<dyn Stream<Item = IncomingMessage> + Send>> {
        crate::transport::broadcast_stream(self.incoming_tx.subscribe(), "lora.messages")
    }

    fn cost_snapshot(&self) -> TransportCost {
        let duty = self.tracker.duty_cycle_remaining_fraction();
        TransportCost {
            estimated_battery_ma: LORA_COST.connected_idle_ma,
            monetary_cost_per_kb: 0.0,
            bandwidth_available_bps: (raw_rate_bps(&DEFAULT_RADIO_PROFILE) as f32 * duty) as u64,
            congestion_level: 0.0,
        }
    }

    fn set_send_priority_hint(&self, priority: MessagePriority) {
        self.last_priority
            .store(priority.as_u8(), Ordering::Relaxed);
    }

    async fn shutdown(&self) -> Result<(), TransportError> {
        self.shutdown_flag.store(true, Ordering::Release);
        // RT-103: take the adapter out of the slot so a post-shutdown attach
        // can never silently resurrect a closed dongle behind Available state.
        let close_err = if let Some(adapter) = self.adapter.write().await.take() {
            adapter.close().await.err()
        } else {
            None
        };
        self.set_state(TransportState::Unavailable);
        if let Some(e) = close_err {
            self.metrics.record_close_failure();
            tracing::warn!(
                target: "iris.transport.lora",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::manager::TransportSelectionRequest;
    use crate::transport::{RegistrationError, TransportManager};
    use futures_util::StreamExt;
    use std::sync::atomic::AtomicU64;

    fn msg(priority: MessagePriority, payload_len: usize) -> SerializedMessage {
        SerializedMessage {
            message_id: MessageId::from([7u8; 16]),
            priority,
            payload: vec![0xAB; payload_len],
        }
    }

    fn fake_tracker(start_ms: u64) -> (Arc<AtomicU64>, DutyCycleTracker) {
        let clock = Arc::new(AtomicU64::new(start_ms));
        let c = clock.clone();
        let tracker = DutyCycleTracker::with_clock(
            ComplianceConfig::default(),
            Box::new(move || c.load(Ordering::Relaxed)),
        )
        .expect("default config valid");
        (clock, tracker)
    }

    fn sim_pair() -> (Arc<SimulatedLoRaAdapter>, Arc<SimulatedLoRaAdapter>) {
        let a = Arc::new(SimulatedLoRaAdapter::new(SimLoRaConfig::default()));
        let b = Arc::new(SimulatedLoRaAdapter::new(SimLoRaConfig::default()));
        SimulatedLoRaAdapter::connect_pair(&a, &b);
        (a, b)
    }

    // ---- compliance (AC-3 pins) ----

    #[test]
    fn compliance_default_pins_wpc_table_i() {
        let cfg = ComplianceConfig::default();
        assert_eq!(cfg.freq_hz, 865_062_500, "IN865_1 primary channel per SPECTRUM_CONSIDERATIONS §2.1");
        assert_eq!(cfg.max_erp_dbm, 14, "25 mW e.r.p. == 14 dBm");
        assert_eq!(cfg.duty_percent, 1.0);
        assert_eq!(cfg.window_ms, 3_600_000);
        assert_eq!(cfg.airtime_budget_ms, 36_000, "1% of one device-hour");
        cfg.validate().expect("default must validate");
    }

    #[test]
    fn compliance_try_new_rejects_non_table_i_configs() {
        let over_duty = ComplianceConfig {
            duty_percent: 2.0,
            ..Default::default()
        };
        assert_eq!(over_duty.validate(), Err(ComplianceError::DutyAboveTableI));

        let over_power = ComplianceConfig {
            max_erp_dbm: 30,
            ..Default::default()
        };
        assert_eq!(
            over_power.validate(),
            Err(ComplianceError::ErpAboveTableI(30))
        );

        let off_band = ComplianceConfig {
            freq_hz: 915_000_000,
            ..Default::default()
        };
        assert_eq!(
            off_band.validate(),
            Err(ComplianceError::BandOutsideSrd(915_000_000))
        );

        let mismatch = ComplianceConfig {
            airtime_budget_ms: 72_000,
            ..Default::default()
        };
        assert!(matches!(
            mismatch.validate(),
            Err(ComplianceError::BudgetMismatch { .. })
        ));

        // The tracker constructor itself refuses them: no override knob.
        assert!(DutyCycleTracker::try_new(over_duty).is_err());
        assert!(DutyCycleTracker::try_new(over_power).is_err());
    }

    // ---- duty-cycle tracker (AC-2/AC-3/AC-4) ----

    #[test]
    fn duty_tracker_accepts_within_budget() {
        let (_clock, tracker) = fake_tracker(3_000_000);
        let win = tracker
            .check_and_consume(MessagePriority::P0, 1_000)
            .expect("first send must fit");
        assert_eq!(win.delay_ms, 1_000);
        let want = 1.0 - 1_000.0 / 36_000.0;
        assert!((win.remaining_fraction - want).abs() < 1e-6);
        assert!((tracker.duty_cycle_remaining_fraction() - want).abs() < 1e-6);
    }

    #[test]
    fn duty_bucket_caps_enforced_proportionally() {
        // RF-3: bucket caps are work-conserving — only enforced under contention
        // from lower-priority demand in the window.

        // P0 cap enforced when P1 demand creates contention.
        let (_clock, t1) = fake_tracker(1_000_000);
        t1.check_and_consume(MessagePriority::P0, 21_600)
            .expect("p0 share fits");
        t1.check_and_consume(MessagePriority::P1, 100)
            .expect("add p1 to create contention");
        let err = t1
            .check_and_consume(MessagePriority::P0, 5_000)
            .expect_err("P0 bucket cap enforced when lower-priority p1 demand exists");
        assert!(err.remaining_fraction > 0.0, "global budget not fully consumed");

        // P0 can borrow unused airtime when no lower-priority demand sits in window.
        let (_c2, t2) = fake_tracker(1_000_000);
        t2.check_and_consume(MessagePriority::P0, 21_600)
            .expect("p0 share fits");
        t2.check_and_consume(MessagePriority::P0, 5_000)
            .expect("no lower-priority contention — P0 borrows from unused shares");
    }

    #[test]
    fn duty_global_exhaustion_refuses_and_reports_window() {
        let (_clock, tracker) = fake_tracker(1_000_000);
        tracker
            .check_and_consume(MessagePriority::P0, 21_600)
            .expect("p0");
        tracker
            .check_and_consume(MessagePriority::P1, 9_000)
            .expect("p1");
        tracker
            .check_and_consume(MessagePriority::P2, 3_600)
            .expect("p2");
        tracker
            .check_and_consume(MessagePriority::P3, 1_800)
            .expect("p3+");
        assert_eq!(tracker.duty_cycle_remaining_fraction(), 0.0);

        let err = tracker
            .check_and_consume(MessagePriority::P3, 1)
            .expect_err("budget fully consumed");
        assert_eq!(err.remaining_fraction, 0.0);
        assert_eq!(err.delay_ms, 3_600_001);
    }

    #[test]
    fn duty_rolling_window_recovers_after_one_hour() {
        let (clock, tracker) = fake_tracker(1_000_000);
        tracker
            .check_and_consume(MessagePriority::P0, 20_000)
            .expect("under the p0 share");
        assert!(tracker.duty_cycle_remaining_fraction() < 0.45);

        // Roll past the window: every record ages out.
        clock.store(1_000_000 + 3_600_000, Ordering::Relaxed);
        assert_eq!(tracker.duty_cycle_remaining_fraction(), 1.0);
        tracker
            .check_and_consume(MessagePriority::P0, 20_000)
            .expect("budget fully restored after the rolling hour");
    }

    #[test]
    fn duty_p0_switch_seam_reports_exhaustion_without_hold() {
        // RF-3: with no lower-priority demand in the window, P0 can borrow
        // unused airtime from the idle lower-priority shares — no refusal.
        let (clock, tracker) = fake_tracker(1_000_000);
        tracker
            .check_and_consume(MessagePriority::P0, 20_000)
            .expect("under the p0 share");
        // No lower-priority demand → work-conserving: P0 uses global budget.
        tracker
            .check_and_consume(MessagePriority::P0, 5_000)
            .expect("P0 borrows unused airtime with no lower-priority contention");
        assert!(tracker.duty_cycle_remaining_fraction() > 0.0);
        clock.store(1_000_000 + 3_600_000, Ordering::Relaxed);
        tracker
            .check_and_consume(MessagePriority::P0, 20_000)
            .expect("window rolled: p0 share restored");
    }

    // ---- RF-7: per-transmission timing (max on-time / min off-time) ----

    #[test]
    fn rf7_max_on_time_exceeded_refuses_regardless_of_budget() {
        // A transmission that is itself too long is refused even though the
        // aggregate window budget has plenty of room — check_transmission_
        // timing runs independently of, and before, check_and_consume.
        let cfg = ComplianceConfig {
            max_ton_ms: 500,
            ..Default::default()
        };
        let tracker =
            DutyCycleTracker::with_clock(cfg, Box::new(|| 0)).expect("valid config");
        let err = tracker
            .check_transmission_timing(600)
            .expect_err("600ms exceeds the 500ms cap");
        assert_eq!(err.reason, TimingRefusalReason::MaxOnTimeExceeded);
        assert_eq!(err.delay_ms, 0, "no wait fixes an inherently-too-long frame");
        // Half the aggregate budget is untouched -- this is not a budget
        // refusal in disguise.
        assert!(tracker.duty_cycle_remaining_fraction() > 0.99);
        tracker
            .check_transmission_timing(500)
            .expect("exactly at the cap must be admitted");
    }

    #[test]
    fn rf7_min_off_time_not_elapsed_refuses_too_soon_after_last_tx() {
        let clock = Arc::new(AtomicU64::new(0));
        let c = clock.clone();
        let cfg = ComplianceConfig {
            min_off_ms: 1_000,
            ..Default::default()
        };
        let tracker = DutyCycleTracker::with_clock(cfg, Box::new(move || c.load(Ordering::Relaxed)))
            .expect("valid config");
        // Admit a 300ms transmission at t=0 -> ends at t=300.
        tracker
            .check_and_consume(MessagePriority::P4, 300)
            .expect("first send admitted");
        // At t=500, only 200ms of the 1000ms off-time has elapsed since the
        // first transmission ended (t=300).
        clock.store(500, Ordering::Relaxed);
        let err = tracker
            .check_transmission_timing(100)
            .expect_err("only 200ms of the 1000ms off-time has elapsed");
        assert_eq!(err.reason, TimingRefusalReason::MinOffTimeNotElapsed);
        assert_eq!(err.delay_ms, 800, "1000ms off-time minus 200ms already elapsed");
        // At t=1300, the full 1000ms off-time (since t=300) has elapsed.
        clock.store(1_300, Ordering::Relaxed);
        tracker
            .check_transmission_timing(100)
            .expect("full off-time has elapsed");
    }

    #[test]
    fn rf7_min_off_time_disabled_by_default() {
        // Default min_off_ms is 0 (disabled) -- back-to-back transmissions
        // are not refused unless a deployment explicitly opts in.
        let (_clock, tracker) = fake_tracker(0);
        tracker
            .check_and_consume(MessagePriority::P4, 100)
            .expect("first send");
        tracker
            .check_transmission_timing(100)
            .expect("immediate back-to-back send is fine with the default config");
    }

    #[tokio::test]
    async fn rf7_try_send_inner_maps_timing_refusals_to_transport_errors() {
        // End-to-end wiring check through the real send() path, not just the
        // tracker method directly.
        let cfg = ComplianceConfig {
            max_ton_ms: 100,
            ..Default::default()
        };
        let tracker = DutyCycleTracker::with_clock(cfg, Box::new(|| 0)).expect("valid config");
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        // A P3/50B frame's airtime comfortably exceeds a 100ms cap.
        let err = t
            .send(&PeerId([8u8; 32]), &msg(MessagePriority::P3, 50))
            .await
            .expect_err("max on-time exceeded");
        assert!(
            matches!(err, TransportError::PolicyDenied(_)),
            "max-on-time violation must map to PolicyDenied (permanent for this frame size), got {err:?}"
        );
    }

    // ---- radio profiles + airtime (AC-7) ----

    #[test]
    fn monotonic_guard_bounds_a_forward_clock_jump() {
        // Regression: only a BACKWARD clamp existed. A forward jump of >= 1 h
        // made prune_locked drop every transmission record, and the backward
        // clamp then wrote the jumped time into last_ms so it could never be
        // walked back — an unlimited, repeatable evasion of the Table-I duty
        // budget, which the module documents as a hard legal floor.
        let mut g = MonotonicGuard::new();
        let base = 1_000_000_000u64;
        assert_eq!(
            g.bound(base),
            base,
            "first observation establishes baseline"
        );
        let jumped = g.bound(base + 3_600_000);
        assert!(
            jumped < base + MONOTONIC_TOLERANCE_MS + 1_000,
            "forward jump must be bounded by real elapsed time, got {}",
            jumped - base
        );
        assert!(g.bound(base - 5_000) >= jumped, "backward step ignored");
    }

    #[test]
    fn airtime_matches_semtech_formula_anchors() {
        // SF9/BW125/CR4/5, 255 B, preamble 8: 305.25 symbols * 4.096 ms.
        assert_eq!(airtime_ms(&DEFAULT_RADIO_PROFILE, 255), 1_251);
        // SF12 (P0): low-data-rate optimization active, 275.25 * 32.768 ms.
        assert_eq!(airtime_ms(&P0_RADIO_PROFILE, 255), 9_020);
        // Tiny payload still costs preamble + header symbols.
        assert_eq!(airtime_ms(&DEFAULT_RADIO_PROFILE, 1), 104);
        // Monotone in payload size.
        assert!(airtime_ms(&DEFAULT_RADIO_PROFILE, 128) < airtime_ms(&DEFAULT_RADIO_PROFILE, 255));
        // Budget binding: one default-profile 255-B frame spends 1251/36000.
        let frac = 1_251.0 / 36_000.0;
        assert!(frac < 0.04 && frac > 0.03);
    }

    #[test]
    fn profile_selection_pins_p0_to_sf12() {
        assert_eq!(profile_for_priority(MessagePriority::P0), P0_RADIO_PROFILE);
        assert_eq!(
            profile_for_priority(MessagePriority::P1),
            DEFAULT_RADIO_PROFILE
        );
        assert_eq!(
            profile_for_priority(MessagePriority::P7),
            DEFAULT_RADIO_PROFILE
        );
        assert_eq!(P0_RADIO_PROFILE.sf, 12);
        assert_eq!(DEFAULT_RADIO_PROFILE.sf, 9);
    }

    #[test]
    fn raw_rate_matches_lora_math() {
        // SF9/BW125/CR4/5 ~= 1757 bps; SF12 ~= 292.97 -> 292 (floor).
        assert_eq!(raw_rate_bps(&DEFAULT_RADIO_PROFILE), 1_757);
        assert_eq!(raw_rate_bps(&P0_RADIO_PROFILE), 292);
    }

    // ---- frame + slip (AC-8) ----

    #[test]
    fn lora_frame_round_trips_all_priorities_with_control_bytes() {
        for p in 0..=7u8 {
            let priority = MessagePriority::from_u8(p).expect("p0..p7");
            let frame = LoRaFrame {
                priority,
                message_id: MessageId::from([0xEE; 16]),
                payload: vec![0xC0, 0xDB, 0x11, 0xDC, 0xDD],
            };
            let bytes = frame.encode().expect("small payload encodes");
            assert_eq!(bytes.len(), HEADER_LEN + 5);
            let decoded = LoRaFrame::decode(&bytes).expect("round-trips");
            assert_eq!(decoded, frame);
        }
    }

    #[test]
    fn lora_frame_defensive_parse_rejects_malformed() {
        // Truncated header.
        assert!(matches!(
            LoRaFrame::decode(&[1, 0, 0]),
            Err(FrameError::Truncated(_))
        ));
        // Bad version.
        let good = LoRaFrame {
            priority: MessagePriority::P2,
            message_id: MessageId::from([1u8; 16]),
            payload: b"hi".to_vec(),
        }
        .encode()
        .unwrap();
        let mut bad_version = good.clone();
        bad_version[0] = 9;
        assert!(matches!(
            LoRaFrame::decode(&bad_version),
            Err(FrameError::BadVersion(9))
        ));
        // Bad priority byte.
        let mut bad_prio = good.clone();
        bad_prio[1] = 200;
        assert!(matches!(
            LoRaFrame::decode(&bad_prio),
            Err(FrameError::BadPriority(200))
        ));
        // Oversized decode input.
        let big = vec![0u8; MAX_FRAME_BYTES + 1];
        assert!(matches!(
            LoRaFrame::decode(&big),
            Err(FrameError::Oversized(_))
        ));
    }

    #[test]
    fn lora_frame_enforces_255_byte_cap() {
        let frame = |n: usize| LoRaFrame {
            priority: MessagePriority::P3,
            message_id: MessageId::from([2u8; 16]),
            payload: vec![0u8; n],
        };
        let ok = frame(MAX_PAYLOAD_BYTES).encode().expect("237 B fits");
        assert_eq!(ok.len(), MAX_FRAME_BYTES);
        assert_eq!(
            ok.len(),
            255,
            "18-B header + 237-B envelope = the P0 wire budget"
        );
        assert!(matches!(
            frame(MAX_PAYLOAD_BYTES + 1).encode(),
            Err(FrameError::PayloadTooLarge(_))
        ));
    }

    #[test]
    fn slip_round_trips_special_bytes() {
        let data = [0xC0, 0x01, 0xDB, 0xDC, 0xDD, 0xFF];
        let framed = encode_slip(&data);
        assert_eq!(framed.last(), Some(&SLIP_END));
        assert_eq!(decode_slip(&framed).expect("round-trips"), data.to_vec());

        // A plain frame with no special bytes survives too.
        let plain = b"hello lora".to_vec();
        assert_eq!(decode_slip(&encode_slip(&plain)).unwrap(), plain);
    }

    #[test]
    fn slip_decode_rejects_malformed() {
        // Dangling escape at end of input.
        assert!(matches!(
            decode_slip(&[0x01, SLIP_ESC]),
            Err(FrameError::DanglingEscape)
        ));
        // Invalid escaped byte.
        assert!(matches!(
            decode_slip(&[SLIP_ESC, 0x41, SLIP_END]),
            Err(FrameError::BadEscape)
        ));
        // Missing END terminator.
        assert!(matches!(decode_slip(b"abc"), Err(FrameError::NoEnd)));
        // Leading delimiters tolerated.
        assert_eq!(
            decode_slip(&[SLIP_END, SLIP_END, 0x01, SLIP_END]).unwrap(),
            vec![0x01]
        );
    }

    // ---- link adapter conformance (AC-6) ----

    #[tokio::test]
    async fn simulated_adapter_pair_round_trips_and_reports_status() {
        let (a, b) = sim_pair();
        a.open().await.expect("open a");
        b.open().await.expect("open b");

        let frame = LoRaFrame {
            priority: MessagePriority::P1,
            message_id: MessageId::from([9u8; 16]),
            payload: b"bridge me".to_vec(),
        };
        let slip = encode_slip(&frame.encode().unwrap());
        a.tx(&slip).await.expect("tx delivers to paired peer");

        let got = b.rx().await.expect("rx works").expect("frame queued");
        assert_eq!(got, slip);
        assert!(b.rx().await.expect("rx works").is_none(), "queue drained");

        let status = a.status().await.expect("status");
        assert_eq!(status.rssi_dbm, -85);
        assert_eq!(status.snr_db, 9);
        assert_eq!(status.battery_pct, 100);
    }

    #[tokio::test]
    async fn simulated_adapter_full_loss_drops_every_frame() {
        let cfg = SimLoRaConfig {
            packet_loss_rate: 1.0,
            seed: 7,
            ..Default::default()
        };
        let a = SimulatedLoRaAdapter::new(cfg.clone());
        let b = SimulatedLoRaAdapter::new(cfg);
        SimulatedLoRaAdapter::connect_pair(&a, &b);
        a.open().await.unwrap();
        b.open().await.unwrap();

        let slip = encode_slip(
            &LoRaFrame {
                priority: MessagePriority::P4,
                message_id: MessageId::from([3u8; 16]),
                payload: b"lost".to_vec(),
            }
            .encode()
            .unwrap(),
        );
        a.tx(&slip).await.expect("loss is silent at tx time");
        assert!(b.rx().await.unwrap().is_none(), "fade swallowed the frame");
    }

    #[tokio::test]
    async fn stub_adapters_report_hardware_gated() {
        let at = AtSerialAdapter::new("/dev/ttyUSB0", 115_200);
        let spi = SpiNativeAdapter::new("/dev/spidev0.0");
        for adapter in [&at as &dyn LoRaLinkAdapter, &spi as &dyn LoRaLinkAdapter] {
            assert_eq!(adapter.open().await, Err(LoRaLinkError::HardwareGated));
            assert_eq!(adapter.tx(&[0xC0]).await, Err(LoRaLinkError::HardwareGated));
            assert_eq!(adapter.rx().await, Err(LoRaLinkError::HardwareGated));
        }
    }

    // ---- link budget (AC-12, SIMULATION_VALIDATED only) ----

    #[test]
    fn link_budget_ranges_are_sane_and_ordered() {
        assert_eq!(SimLinkBudget::sensitivity_dbm(9), Some(-129.0));
        assert_eq!(SimLinkBudget::sensitivity_dbm(12), Some(-136.0));
        assert_eq!(SimLinkBudget::sensitivity_dbm(3), None);

        let r9 = SimLinkBudget::max_range_km(&DEFAULT_RADIO_PROFILE, 14);
        let r12 = SimLinkBudget::max_range_km(&P0_RADIO_PROFILE, 14);
        assert!(r9 > 2.9 && r9 < 3.2, "sf9 urban range {r9} km");
        assert!(r12 > 4.6 && r12 < 5.1, "sf12 urban range {r12} km");
        assert!(r12 > r9, "+7 dB sensitivity must extend the range");

        assert_eq!(
            SimLinkBudget::delivery_probability(&DEFAULT_RADIO_PROFILE, 14, r9 * 0.5),
            1.0
        );
        let far = SimLinkBudget::delivery_probability(&DEFAULT_RADIO_PROFILE, 14, 10.0);
        assert!(far <= 0.05, "way past the margin fades to the floor");
        assert!(SimLinkBudget::MAX_RANGE_KM >= r12);

        // RF-15: outside the model validity window → None / 0.0, not clamped.
        assert!(
            SimLinkBudget::okumura_hata_urban_loss_db(866.0, 500.0).is_none(),
            "500 km is outside model validity"
        );
        assert_eq!(
            SimLinkBudget::delivery_probability(&DEFAULT_RADIO_PROFILE, 14, 500.0),
            0.0,
            "far-field delivery_probability returns 0.0, not the 20 km floor"
        );
    }

    // ---- backlog queue (AC-5) ----

    #[test]
    fn backlog_push_deferred_over_cap_retains_held_traffic_sat_rt104() {
        let q: BacklogQueue<u8> = BacklogQueue::new();
        for i in 0..MAX_BACKLOG_ENTRIES {
            assert!(q.push(MessagePriority::P4, (i % 251) as u8 + 1));
        }
        assert!(!q.push(MessagePriority::P4, 7), "cap refuses new pushes");
        // Deferred re-insert ignores the cap: nothing already-held is dropped
        // when concurrent enqueues refill during a drain.
        q.push_deferred(BacklogEntry {
            priority: MessagePriority::P4,
            enqueued_at: Instant::now(),
            next_attempt_at: Instant::now(),
            item: 7u8,
        });
        assert_eq!(q.len(), MAX_BACKLOG_ENTRIES + 1);
        // Drain still works over the temporarily-over-cap queue.
        let mut drained = 0;
        while q.pop_highest().is_some() {
            drained += 1;
        }
        assert_eq!(drained, MAX_BACKLOG_ENTRIES + 1);
    }

    #[test]
    fn rf13_p0_evicts_lowest_priority_when_queue_is_full() {
        let q: BacklogQueue<u8> = BacklogQueue::new();
        // Fill the queue with P4 entries.
        for i in 0..MAX_BACKLOG_ENTRIES {
            assert!(q.push(MessagePriority::P4, (i % 251) as u8 + 1));
        }
        assert_eq!(q.len(), MAX_BACKLOG_ENTRIES);
        // A P0 (higher urgency) push must succeed by evicting one P4.
        assert!(q.push(MessagePriority::P0, 0), "P0 must evict lowest-priority P4");
        assert_eq!(q.len(), MAX_BACKLOG_ENTRIES, "length unchanged: one evicted, one added");
        // The front must be our P0 entry.
        let top = q.pop_highest().expect("queue non-empty");
        assert_eq!(top.priority, MessagePriority::P0, "P0 is highest-priority");
        assert_eq!(top.item, 0u8);
        // A same-priority push at cap must still be refused (no eviction when
        // incoming priority is not strictly higher).
        let q2: BacklogQueue<u8> = BacklogQueue::new();
        for i in 0..MAX_BACKLOG_ENTRIES {
            assert!(q2.push(MessagePriority::P4, (i % 251) as u8 + 1));
        }
        assert!(!q2.push(MessagePriority::P4, 99), "same-priority push still refused at cap");
    }

    #[test]
    fn backlog_drains_in_priority_order_fifo_within_class() {
        let q: BacklogQueue<u8> = BacklogQueue::new();
        q.push(MessagePriority::P4, 41);
        q.push(MessagePriority::P0, 1);
        q.push(MessagePriority::P2, 21);
        q.push(MessagePriority::P0, 2);
        q.push(MessagePriority::P2, 22);

        let order: Vec<u8> = std::iter::from_fn(|| q.pop_highest().map(|e| e.item)).collect();
        assert_eq!(order, vec![1, 2, 21, 22, 41]);
        assert!(q.is_empty());
    }

    // ---- transport integration (AC-1/AC-4/AC-5/AC-13/AC-15) ----

    #[tokio::test]
    async fn send_consumes_duty_then_refuses_then_recovers_after_roll() {
        let (_clock, tracker) = fake_tracker(5_000_000);
        let tracker = Arc::new(tracker);
        let t = LoRaTransport::with_tracker("lora-0", tracker.clone());
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        assert_eq!(t.state(), TransportState::Available);

        // RF-3 (work-conserving shares): Bulk (P3) is the lowest priority
        // LoRa accepts, so it's never "contended" — it can always borrow
        // from idle P0/P1/P2 shares up to the global 36 s legal cap.
        // To test that refusal DOES eventually happen, exhaust the global
        // budget directly via the tracker, leaving only enough for 2 P3
        // sends. A 150-B P3 frame costs 862 ms; 2 × 862 = 1724 ms.
        let global_cap_ms = 36_000u64;
        let frame_ms = 862u64;
        let left_for_sends = 2 * frame_ms; // 1724 ms
        tracker
            .check_and_consume(MessagePriority::P0, global_cap_ms - left_for_sends)
            .expect("prime global budget — leaves 1724 ms");

        let peer = PeerId([2u8; 32]);
        t.send(&peer, &msg(MessagePriority::P3, 150))
            .await
            .expect("first send fits remaining budget");
        t.send(&peer, &msg(MessagePriority::P3, 150))
            .await
            .expect("second send fits exactly");
        let err = t
            .send(&peer, &msg(MessagePriority::P3, 150))
            .await
            .expect_err("global budget exhausted");
        // RF-5: duty refusal now carries a retry time (RateLimited), not
        // the generic pool-saturation Busy.
        assert!(matches!(err, TransportError::RateLimited { .. }));
        assert_eq!(t.metrics_snapshot().duty_refusals, 1);

        // Window rolls -> budget restores; the same send succeeds again.
        _clock.store(5_000_000 + 3_600_000, Ordering::Relaxed);
        t.send(&peer, &msg(MessagePriority::P3, 150))
            .await
            .expect("rolling hour restored the budget");
        let m = t.metrics_snapshot();
        assert_eq!(m.packets_tx, 3);
        assert_eq!(m.airtime_consumed_ms, 3 * frame_ms);
    }

    #[tokio::test]
    async fn duty_bills_full_encoded_frame_not_just_payload_rt101() {
        let (_clock, tracker) = fake_tracker(0);
        let tracker = Arc::new(tracker);
        let t = LoRaTransport::with_tracker("lora-0", tracker.clone());
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");

        // RT-101 regression anchor: a 1-B payload rides a 19-B LoRa frame.
        // airtime(SF9, BW125, PL=19) = 186 ms — NOT the payload-only value.
        // RF-10: P3 replaces the old P4+ example (P4+ no longer rides LoRa).
        //
        // RF-3 (work-conserving shares): Bulk (P3) is the lowest priority
        // LoRa accepts — never contended, so it borrows from idle shares up
        // to the global 36 s cap. To test that the global limit is enforced,
        // prime the budget via tracker BEFORE any P3 sends so the queue is
        // empty and P0 is uncontended (can use the full global budget).
        // Leave room for exactly 5 P3 frames (5 × 186 = 930 ms).
        let frame_ms = 186u64; // 1-B P3 frame, SF9
        let global_cap_ms = 36_000u64;
        let total_sends = 5u64;
        let to_prime = global_cap_ms - total_sends * frame_ms; // 36000 - 930 = 35070 ms
        tracker
            .check_and_consume(MessagePriority::P0, to_prime)
            .expect("prime: queue is empty so P0 borrows from global budget");

        let peer = PeerId([9u8; 32]);
        // First send verifies RT-101 frame-overhead billing.
        t.send(&peer, &msg(MessagePriority::P3, 1))
            .await
            .expect("first minimal frame fits");
        assert_eq!(
            t.metrics_snapshot().airtime_consumed_ms, frame_ms,
            "RT-101: frame overhead (18 bytes header) must be included in duty billing, not payload bytes only"
        );

        // 4 more frames fit; the 5th must refuse (global cap reached).
        let mut admitted = 0;
        for _ in 0..10 {
            if t.send(&peer, &msg(MessagePriority::P3, 1)).await.is_ok() {
                admitted += 1;
            }
        }
        assert_eq!(admitted, (total_sends - 1) as usize, "global budget limits sends after priming");
        let total = t.metrics_snapshot().airtime_consumed_ms;
        assert!(
            total <= global_cap_ms,
            "total transport airtime {total} ms must not exceed the 36 s Table-I legal cap"
        );
    }

    #[tokio::test]
    async fn tx_failure_refunds_duty_reservation_rt104() {
        let (_clock, tracker) = fake_tracker(0);
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a.clone()).await.expect("adapter opens");

        // Kill the link AFTER attach: the next send consumes duty, then the
        // adapter refuses -> refund + failure metric, no budget burn.
        // RF-10: P3/100 B, not P4/237 B -- P4+ is now rejected at the
        // priority gate before ever reaching the tx attempt this test
        // exercises, and 237 B exceeds every priority's new payload cap.
        a.close().await.expect("close sim link");
        let peer = PeerId([10u8; 32]);
        let err = t
            .send(&peer, &msg(MessagePriority::P3, 100))
            .await
            .expect_err("closed link fails the tx");
        // MG-39/41: a closed link is distinguishable from a genuine I/O
        // failure now — `LoRaLinkError::Closed` maps to `NotConnected`.
        assert!(matches!(err, TransportError::NotConnected));
        let m = t.metrics_snapshot();
        assert_eq!(m.tx_failures, 1);
        assert_eq!(m.packets_tx, 0);
        assert_eq!(m.airtime_consumed_ms, 0);
        assert_eq!(
            t.duty_cycle_remaining(),
            1.0,
            "reservation refunded — no airtime actually used"
        );
    }

    #[tokio::test]
    async fn malformed_inbound_frame_does_not_abort_poll_batch_rt105() {
        let ta = LoRaTransport::new("lora-a");
        let tb = LoRaTransport::new("lora-b");
        let (a, b) = sim_pair();
        ta.attach_adapter(a.clone()).await.expect("adapter opens");
        tb.attach_adapter(b.clone()).await.expect("adapter opens");

        // Inject garbage then a valid frame toward tb: a.tx lands in b's
        // inbound queue (pair topology), which is what tb polls.
        a.tx(&[0xFF, 0x00, SLIP_END]).await.expect("garbage queued");
        let good = encode_slip(
            &LoRaFrame {
                priority: MessagePriority::P3,
                message_id: MessageId::from([4u8; 16]),
                payload: b"survivor".to_vec(),
            }
            .encode()
            .unwrap(),
        );
        a.tx(&good).await.expect("good frame queued");

        // The malformed prefix must not deny the legitimate frame.
        assert_eq!(tb.poll_inbound().await.expect("poll"), 1);
        assert_eq!(tb.metrics_snapshot().malformed_rx, 1);
        assert_eq!(tb.metrics_snapshot().packets_rx, 1);
    }

    #[tokio::test]
    async fn rf9_poll_inbound_is_bounded_not_an_unconditional_drain() {
        // A continuously transmitting node (hostile or a stuck neighbour)
        // must not be able to make a single poll_inbound() call process an
        // unbounded backlog -- that used to block the caller's task
        // indefinitely as long as rx() kept returning Some.
        let ta = LoRaTransport::new("lora-a");
        let tb = LoRaTransport::new("lora-b");
        let (a, b) = sim_pair();
        ta.attach_adapter(a.clone()).await.expect("adapter opens");
        tb.attach_adapter(b.clone()).await.expect("adapter opens");

        let total = MAX_FRAMES_PER_POLL * 2 + 3; // deliberately not a clean multiple
        for i in 0..total {
            let frame = encode_slip(
                &LoRaFrame {
                    priority: MessagePriority::P3,
                    message_id: MessageId::from([(i % 256) as u8; 16]),
                    payload: b"flood".to_vec(),
                }
                .encode()
                .unwrap(),
            );
            a.tx(&frame).await.expect("frame queued");
        }

        let first = tb.poll_inbound().await.expect("first poll");
        assert_eq!(
            first, MAX_FRAMES_PER_POLL,
            "one call must process at most the per-call budget, not the whole backlog"
        );
        // The caller re-polls for the rest -- nothing was lost, just deferred.
        let second = tb.poll_inbound().await.expect("second poll");
        assert_eq!(second, MAX_FRAMES_PER_POLL);
        let third = tb.poll_inbound().await.expect("third poll");
        assert_eq!(third, total - 2 * MAX_FRAMES_PER_POLL, "remainder drains on the next call");
        assert_eq!(tb.metrics_snapshot().packets_rx, total as u64);
    }

    #[tokio::test]
    async fn dead_dongle_attach_is_rejected_not_available_rt103() {
        let t = LoRaTransport::new("lora-0");
        let gated = AtSerialAdapter::new("COM3", 115_200);
        let err = t
            .attach_adapter(Arc::new(gated))
            .await
            .expect_err("HardwareGated open must fail attach");
        assert_eq!(err, LoRaLinkError::HardwareGated);
        assert_eq!(t.state(), TransportState::Unavailable);
    }

    #[tokio::test]
    async fn rf1_attach_rejects_module_reporting_wrong_frequency() {
        let t = LoRaTransport::new("lora-0");
        let (a, _b) = sim_pair();
        // Simulate a module that booted already configured off the
        // compliance-pinned frequency (RF-1's own trigger scenario: an
        // EU-default module boots at 868 MHz, not the 866 MHz Table-I pin).
        a.open().await.expect("test setup: open before preset");
        a.configure(&RadioProfile { freq_hz: 868_000_000, ..DEFAULT_RADIO_PROFILE }, 14)
            .await
            .expect("test setup: preset a non-compliant frequency");
        let err = t
            .attach_adapter(a)
            .await
            .expect_err("frequency mismatch must refuse attach");
        assert!(matches!(err, LoRaLinkError::Io(_)));
        assert_eq!(
            t.state(),
            TransportState::Unavailable,
            "a rejected module must never be promoted to Available"
        );
    }

    #[tokio::test]
    async fn rf1_attach_rejects_module_reporting_excessive_power() {
        let t = LoRaTransport::new("lora-0");
        let (a, _b) = sim_pair();
        a.open().await.expect("test setup: open before preset");
        a.configure(&DEFAULT_RADIO_PROFILE, 27) // above the 14 dBm Table-I ceiling
            .await
            .expect("test setup: preset excessive power");
        let err = t
            .attach_adapter(a)
            .await
            .expect_err("power exceeding the compliance ceiling must refuse attach");
        assert!(matches!(err, LoRaLinkError::Io(_)));
        assert_eq!(t.state(), TransportState::Unavailable);
    }

    #[tokio::test]
    async fn rf1_try_send_inner_configures_the_module_for_the_actual_profile_sent() {
        // End-to-end: a P0 send must leave the module confirmed-configured
        // for P0_RADIO_PROFILE (SF12), not silently left at whatever it
        // reported at attach time (DEFAULT_RADIO_PROFILE, SF9) -- this is
        // RF-1's core finding: the duty tracker must not bill airtime for
        // a profile the module was never actually told to use.
        let t = LoRaTransport::new("lora-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a.clone()).await.expect("adapter opens");
        let (initial_profile, _) = a.read_config().await.expect("read back initial config");
        assert_eq!(
            initial_profile, DEFAULT_RADIO_PROFILE,
            "attach leaves the module at whatever it already reported"
        );
        t.send(&PeerId([11u8; 32]), &msg(MessagePriority::P0, 60))
            .await
            .expect("p0 send");
        let (after_p0, power) = a.read_config().await.expect("read back post-send config");
        assert_eq!(
            after_p0, P0_RADIO_PROFILE,
            "try_send_inner must configure the module for the profile it actually bills airtime for"
        );
        assert_eq!(power, 14);
    }

    #[tokio::test]
    async fn p0_send_rides_sf12_airtime_accounting() {
        let (_clock, tracker) = fake_tracker(0);
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        // RF-10: P0's payload cap is now 60 B (was 237, the pre-fix uniform
        // ceiling) -- airtime(SF12, PL=78) recomputed for the 60-B payload
        // + 18-B header, not the old 255-B full-frame anchor.
        t.send(&PeerId([1u8; 32]), &msg(MessagePriority::P0, 60))
            .await
            .expect("p0 within its 60% share");
        assert_eq!(t.metrics_snapshot().airtime_consumed_ms, 3_285);
        assert_eq!(t.send_priority_hint(), MessagePriority::P4, "hint unset");
        t.set_send_priority_hint(MessagePriority::P0);
        assert_eq!(t.send_priority_hint(), MessagePriority::P0);
    }

    #[tokio::test]
    async fn oversize_payload_is_rejected_up_front() {
        let t = LoRaTransport::new("lora-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        // RF-10: P3's new 150-B cap is the binding oversize boundary now,
        // not the old uniform 255-B wire limit -- P4+ would hit
        // PolicyDenied first, which is a different (also-tested-elsewhere)
        // failure mode than "too big for this priority's frame".
        let err = t
            .send(&PeerId([3u8; 32]), &msg(MessagePriority::P3, 151))
            .await
            .expect_err(">150 B exceeds P3's LoRa payload cap");
        assert!(matches!(err, TransportError::MessageTooLarge { .. }));
    }

    #[tokio::test]
    async fn hot_plug_attach_detach_and_shutdown_lifecycle() {
        let t = LoRaTransport::new("lora-0");
        assert_eq!(t.state(), TransportState::Unavailable, "no dongle yet");

        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        assert_eq!(t.state(), TransportState::Available);

        t.detach_adapter().await;
        assert_eq!(t.state(), TransportState::Unavailable);

        let (a2, _b2) = sim_pair();
        t.attach_adapter(a2).await.expect("adapter opens");
        t.shutdown().await.expect("clean shutdown");
        assert_eq!(t.state(), TransportState::Unavailable);
        let err = t
            .send(&PeerId([4u8; 32]), &msg(MessagePriority::P3, 10))
            .await
            .expect_err("shut down refuses sends");
        assert!(matches!(err, TransportError::ShuttingDown));
    }

    #[tokio::test]
    async fn backlog_holds_while_exhausted_and_drains_after_recovery() {
        let (clock, tracker) = fake_tracker(0);
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        let peer = PeerId([5u8; 32]);

        // RF-10: P2's payload cap is now 120 B (was 237). Send full-size P2
        // frames until the Location bucket (10% = 3600 ms) refuses one,
        // rather than hardcoding a frame count derived from the old,
        // larger payload size.
        let mut sent = 0;
        loop {
            match t.send(&peer, &msg(MessagePriority::P2, 120)).await {
                Ok(_) => sent += 1,
                Err(_) => break,
            }
            assert!(
                sent < 100,
                "bucket should exhaust well before this many full-size frames"
            );
        }
        assert!(sent >= 1, "at least one full P2 frame must fit the 10% share");

        // Hold a message that cannot fit the now-exhausted share.
        t.enqueue_backlog(msg(MessagePriority::P2, 120))
            .expect("fits the queue");

        let drained = t.drain_backlog(4).await;
        assert!(drained.is_empty(), "bucket short: nothing drains yet");
        assert_eq!(t.backlog_len(), 1, "refused entry stays queued");

        clock.store(3_600_000, Ordering::Relaxed);
        let drained = t.drain_backlog(4).await;
        assert_eq!(drained.len(), 1, "budget recovered -> backlog drains");
        assert_eq!(drained[0].0.priority, MessagePriority::P2);
        assert_eq!(t.backlog_len(), 0);
    }

    #[tokio::test]
    async fn end_to_end_frame_crosses_the_simulated_link() {
        let ta = LoRaTransport::new("lora-a");
        let tb = LoRaTransport::new("lora-b");
        let (a, b) = sim_pair();
        ta.attach_adapter(a).await.expect("adapter opens");
        tb.attach_adapter(b).await.expect("adapter opens");

        let mut inbox = tb.incoming_messages();
        let sent = msg(MessagePriority::P3, 50);
        let peer = PeerId([6u8; 32]);
        let receipt = ta.send(&peer, &sent).await.expect("cross-link send");
        assert_eq!(receipt.peer_id, peer);

        assert_eq!(tb.poll_inbound().await.expect("poll"), 1);
        let got = tokio::time::timeout(Duration::from_millis(200), inbox.next())
            .await
            .expect("message arrives")
            .expect("stream yields");
        assert_eq!(got.payload, sent.payload);
        assert_eq!(got.transport_id, "lora-b");
        assert_eq!(got.peer_id, PeerId([0u8; 32]), "zero fallback attribution");
        assert_eq!(tb.metrics_snapshot().packets_rx, 1);
    }

    // ---- registry integration (AC-1/AC-13) ----

    #[tokio::test]
    async fn manager_registers_selects_and_hot_unplugs_lora() {
        let mgr = TransportManager::new();
        let t = Arc::new(LoRaTransport::new("lora-0"));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");

        mgr.register(t.clone()).await.expect("hot-plug register");
        assert!(matches!(
            mgr.register(t.clone()).await,
            Err(RegistrationError::DuplicateId)
        ));

        let req_ok = TransportSelectionRequest {
            target_peer: None,
            message_size: 200,
            priority: MessagePriority::P3,
            max_latency_ms: None,
            prefer_low_cost: false,
            multipath: false,
            fragmentable: false,
        };
        assert!(mgr
            .select_transports(&req_ok)
            .await
            .iter()
            .any(|r| r.transport_id.as_str() == "lora-0"));

        // >255-B unfragmentable requests must exclude LoRa (manager gating).
        let req_big = TransportSelectionRequest {
            message_size: 400,
            fragmentable: false,
            ..req_ok
        };
        assert!(!mgr
            .select_transports(&req_big)
            .await
            .iter()
            .any(|r| r.transport_id.as_str() == "lora-0"));

        // Caps row: SubGHz-class hardware + WPC band string (AC-1). The
        // reported envelope capacity is 237 B (255-B PHY minus IRIS header).
        let caps = t.capabilities();
        assert_eq!(caps.max_message_size, MAX_PAYLOAD_BYTES);
        assert_eq!(caps.max_message_size, 237);
        assert!(caps.requires_special_hardware);
        assert!(!caps.supports_multicast);
        assert_eq!(caps.cost_class, TransportCostClass::Free);
        assert!(caps
            .regulatory_band
            .as_deref()
            .is_some_and(|b| b.contains("853(E)")));

        // Hot unplug: deregister shuts the transport down cleanly (AC-13).
        mgr.deregister(&TransportId::from("lora-0"))
            .await
            .expect("deregister");
        assert_eq!(t.state(), TransportState::Unavailable);
        assert!(!mgr.list().await.iter().any(|id| id.as_str() == "lora-0"));
    }
}
