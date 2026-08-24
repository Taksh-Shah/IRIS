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
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
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
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        ComplianceConfig {
            freq_hz: 866_000_000,
            max_erp_dbm: 14,
            duty_percent: 1.0,
            window_ms: 3_600_000,
            airtime_budget_ms: 36_000,
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
    pub at_unix_ms: u64,
    pub delay_ms: u64,
    pub remaining_fraction: f32,
}

/// Why a LoRa send was refused: the 1% duty budget (or this bucket's share)
/// is exhausted this window. Carries the next legal send time so the routing
/// layer can schedule around it (RFC 9171 §6.9 analogy, RES-0008 R7) — never
/// a panic, never a hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetExhausted {
    pub at_unix_ms: u64,
    pub delay_ms: u64,
    pub remaining_fraction: f32,
}

type NowFn = Box<dyn Fn() -> u64 + Send + Sync>;

#[derive(Debug, Clone, Copy)]
struct TxRecord {
    start_ms: u64,
    airtime_ms: u64,
    bucket: DutyBucket,
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
                let mut g = guard.lock().unwrap();
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

    /// RT-104: refund a reservation that never reached the air (TX failed
    /// after `check_and_consume` admitted it). Removes the NEWEST record
    /// matching class+airtime. Under concurrent senders an identical
    /// (class, airtime) reservation may race — the worst case is one
    /// reservation of refund ambiguity, never a budget bypass (refund only
    /// ever REMOVES usage).
    pub fn refund(&self, class: MessagePriority, airtime_ms: u64) -> bool {
        let bucket = DutyBucket::from_priority(class);
        let mut q = self.tx.lock().unwrap();
        for idx in (0..q.len()).rev() {
            if q[idx].bucket == bucket && q[idx].airtime_ms == airtime_ms {
                q.remove(idx);
                return true;
            }
        }
        false
    }

    /// Fraction of the per-device hourly budget still unused, in [0, 1].
    pub fn duty_cycle_remaining_fraction(&self) -> f32 {
        let mut q = self.tx.lock().unwrap();
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

        let mut q = self.tx.lock().unwrap();
        prune_locked(&mut q, window, now);
        let (used, by_bucket) = usage_locked(&q);
        let global_remaining = budget.saturating_sub(used);
        let cap = bucket.cap_ms(budget);
        let bucket_remaining = cap.saturating_sub(by_bucket[bucket_index(bucket)]);

        if airtime_ms <= global_remaining && airtime_ms <= bucket_remaining {
            q.push_back(TxRecord {
                start_ms: now,
                airtime_ms,
                bucket,
            });
            return Ok(NextWindow {
                at_unix_ms: now + airtime_ms,
                delay_ms: airtime_ms,
                remaining_fraction: remaining_fraction(used + airtime_ms, budget),
            });
        }

        // RT-107: find the earliest record expiry that satisfies BOTH
        // constraints (walking every record — an absolute-oversize request
        // finds none and waits out the full window).
        let mut g_left = global_remaining;
        let mut b_left = bucket_remaining;
        let bi = bucket_index(bucket);
        let mut chosen: Option<u64> = None;
        for r in q.iter() {
            let expires = r.start_ms + window;
            g_left += r.airtime_ms;
            if bucket_index(r.bucket) == bi {
                b_left += r.airtime_ms;
            }
            if airtime_ms <= g_left && airtime_ms <= b_left {
                chosen = Some(expires + 1 - now);
                break;
            }
        }
        let delay = chosen.unwrap_or(window + 1);
        Err(BudgetExhausted {
            at_unix_ms: now + delay,
            delay_ms: delay,
            remaining_fraction: remaining_fraction(used, budget),
        })
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

/// Default link profile: SF9 / BW125 kHz / CR4/5 @ 866.0 MHz.
pub const DEFAULT_RADIO_PROFILE: RadioProfile = RadioProfile {
    sf: 9,
    bandwidth_khz: 125,
    coding_rate_denom: 5,
    freq_hz: 866_000_000,
    preamble_symbols: 8,
};

/// P0 emergency profile: SF12 / BW125 kHz / CR4/5 (−136 dBm sensitivity,
/// AN1200.13). Same band; ≈4× the airtime of SF9.
pub const P0_RADIO_PROFILE: RadioProfile = RadioProfile {
    sf: 12,
    bandwidth_khz: 125,
    coding_rate_denom: 5,
    freq_hz: 866_000_000,
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
    let de = if profile.sf >= 11 && profile.bandwidth_khz == 125 {
        1.0
    } else {
        0.0
    };
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
}

impl std::fmt::Display for LoRaLinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoRaLinkError::HardwareGated => write!(f, "link hardware-gated (BLK-0005/GAP-004)"),
            LoRaLinkError::Closed => write!(f, "link closed"),
            LoRaLinkError::Io(m) => write!(f, "link io: {m}"),
        }
    }
}

impl std::error::Error for LoRaLinkError {}

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
            }),
        }))
    }

    /// Wire two adapters to each other (both directions).
    pub fn connect_pair(a: &SimulatedLoRaAdapter, b: &SimulatedLoRaAdapter) {
        a.0.state.lock().unwrap().peer = Some(Arc::clone(&b.0));
        b.0.state.lock().unwrap().peer = Some(Arc::clone(&a.0));
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
            let mut st = self.0.state.lock().unwrap();
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
            peer.state.lock().unwrap().inbound.push_back(pending);
        }
        // An unpaired open link still transmits (raw radio broadcasts to nobody).
        Ok(())
    }

    async fn rx(&self) -> Result<Option<Vec<u8>>, LoRaLinkError> {
        if !self.0.opened.load(Ordering::Acquire) {
            return Err(LoRaLinkError::Closed);
        }
        let mut st = self.0.state.lock().unwrap();
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
    /// 150–1500 MHz carriers (866 qualifies). Distance clamped to validity.
    pub fn okumura_hata_urban_loss_db(freq_mhz: f64, dist_km: f64) -> f64 {
        let d = dist_km.clamp(Self::MIN_RANGE_KM, Self::MAX_RANGE_KM);
        let lf = freq_mhz.log10();
        let lhb = Self::BASE_STATION_M.log10();
        let a_hm = (1.1 * lf - 0.7) * Self::MOBILE_M - (1.56 * lf - 0.8);
        let c1 = 69.55 + 26.16 * lf - 13.82 * lhb - a_hm;
        let slope = 44.9 - 6.55 * lhb;
        c1 + slope * d.log10()
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
    /// Deterministic pure function (SIMULATION_VALIDATED only).
    pub fn delivery_probability(profile: &RadioProfile, tx_dbm: i8, dist_km: f64) -> f32 {
        let sens = Self::sensitivity_dbm(profile.sf).unwrap_or(-129.0);
        let freq_mhz = profile.freq_hz as f64 / 1.0e6;
        let loss = Self::okumura_hata_urban_loss_db(freq_mhz, dist_km);
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
    /// RT-104: sends whose duty reservation was refunded after a TX failure.
    tx_failures: AtomicU64,
    /// RT-105: inbound frames dropped for malformed SLIP/frame encoding.
    malformed_rx: AtomicU64,
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
    pub malformed_rx: u64,
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

    fn record_malformed_rx(&self) {
        self.malformed_rx.fetch_add(1, Ordering::Relaxed);
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
            malformed_rx: self.malformed_rx.load(Ordering::Relaxed),
        }
    }
}

// ==== 7. Backlog queue (REQ-ROUTE-C-003 / D-5, DEC-LORA-0003) ====

/// One held send: its priority plus the payload waiting for budget recovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogEntry<T> {
    pub priority: MessagePriority,
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
    /// Returns false when at capacity (RT-106 memory bound).
    pub fn push(&self, priority: MessagePriority, item: T) -> bool {
        let mut g = self.entries.lock().unwrap();
        if g.len() >= MAX_BACKLOG_ENTRIES {
            return false;
        }
        let pos = g.partition_point(|e| e.priority <= priority);
        g.insert(pos, BacklogEntry { priority, item });
        true
    }

    /// SAT-RT-104 lesson (mirrored from the satellite transport review):
    /// re-insert an item that was ALREADY queued (drain deferral) — ignores
    /// the depth cap so concurrent enqueues during a drain can never silently
    /// drop held traffic. May temporarily exceed [`MAX_BACKLOG_ENTRIES`].
    pub fn push_deferred(&self, priority: MessagePriority, item: T) {
        let mut g = self.entries.lock().unwrap();
        let pos = g.partition_point(|e| e.priority <= priority);
        g.insert(pos, BacklogEntry { priority, item });
    }

    /// Pop the highest-priority entry (front of the sorted vec).
    pub fn pop_highest(&self) -> Option<BacklogEntry<T>> {
        let mut g = self.entries.lock().unwrap();
        if g.is_empty() {
            None
        } else {
            Some(g.remove(0))
        }
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap().len()
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
}

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
        }
    }

    fn set_state(&self, state: TransportState) {
        self.state.store(state);
        let _ = self.state_tx.send(TransportStateEvent {
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
        adapter.open().await?;
        *self.adapter.write().await = Some(adapter);
        if self.state.load() < TransportState::Available {
            self.set_state(TransportState::Available);
        }
        Ok(())
    }

    /// Dongle unplug: close + drop the adapter; the transport goes
    /// Unavailable (`TransportManager::deregister` removes it from selection).
    pub async fn detach_adapter(&self) {
        if let Some(adapter) = self.adapter.write().await.take() {
            let _ = adapter.close().await;
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
        if msg.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(TransportError::Protocol(format!(
                "payload {} B exceeds LoRa {}-B frame capacity (fragment upstream)",
                msg.payload.len(),
                MAX_PAYLOAD_BYTES
            )));
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
        if msg.payload.len() > MAX_PAYLOAD_BYTES {
            return Err(TransportError::Protocol(format!(
                "payload {} B exceeds LoRa {}-B frame capacity (fragment upstream)",
                msg.payload.len(),
                MAX_PAYLOAD_BYTES
            )));
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
        if let Err(be) = self.tracker.check_and_consume(msg.priority, airtime) {
            tracing::debug!(
                target: "iris.transport.lora",
                duty_refusal = true,
                retry_in_ms = be.delay_ms,
                "send refused by 1% duty budget; re-route or P0-switch"
            );
            self.metrics.record_refusal();
            return Err(TransportError::Busy);
        }
        let slip = encode_slip(&encoded);
        if let Err(e) = adapter.tx(&slip).await {
            // RT-104: the reservation never reached the air — refund it so a
            // flapping dongle cannot incinerate the legal hourly budget.
            self.tracker.refund(msg.priority, airtime);
            self.metrics.record_tx_failure();
            tracing::debug!(target: "iris.transport.lora", "tx failed after duty reservation; budget refunded");
            return Err(TransportError::Io(format!("lora link tx: {e}")));
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
        for _ in 0..max {
            let Some(entry) = self.backlog.pop_highest() else {
                break;
            };
            // Backlog entries carry no destination (raw-LoRa is broadcast at
            // the PHY): receipts use the zero-PeerId fallback convention.
            match self.try_send_inner(&PeerId([0u8; 32]), &entry.item).await {
                Ok(receipt) => drained.push((entry.item, receipt)),
                Err(_) => deferred.push(entry),
            }
        }
        for entry in deferred {
            // SAT-RT-104: deferred items were already queued once — re-insert
            // over-capacity so concurrent enqueues can never drop them.
            self.backlog.push_deferred(entry.priority, entry.item);
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
        loop {
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
                            let _ = self.incoming_tx.send(IncomingMessage {
                                peer_id: PeerId([0u8; 32]),
                                transport_id: self.id.as_str().to_string(),
                                payload: frame.payload,
                                received_at: Instant::now(),
                            });
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
                Err(e) => return Err(TransportError::Io(format!("lora link rx: {e}"))),
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
        crate::transport::broadcast_stream(self.state_tx.subscribe())
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
            .map_err(|e| TransportError::Io(format!("lora link status probe: {e}")))?;
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
        crate::transport::broadcast_stream(self.incoming_tx.subscribe())
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
        if let Some(adapter) = self.adapter.write().await.take() {
            let _ = adapter.close().await;
        }
        self.set_state(TransportState::Unavailable);
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
        assert_eq!(cfg.freq_hz, 866_000_000);
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
        let (_clock, tracker) = fake_tracker(1_000_000);
        // Bulk cap = 5% of 36 s = 1800 ms.
        tracker
            .check_and_consume(MessagePriority::P4, 1_800)
            .expect("exactly the bulk share fits");
        let err = tracker
            .check_and_consume(MessagePriority::P4, 1_000)
            .expect_err("bulk bucket exhausted even though global budget is free");
        assert!(err.remaining_fraction > 0.9, "global mostly untouched");
        assert_eq!(err.delay_ms, 3_600_001, "window frees at oldest expiry");

        // Emergency cap = 60% = 21,600 ms.
        let (_c2, t2) = fake_tracker(1_000_000);
        t2.check_and_consume(MessagePriority::P0, 21_600)
            .expect("p0 share fits");
        assert!(t2.check_and_consume(MessagePriority::P0, 5_000).is_err());
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
        let (clock, tracker) = fake_tracker(1_000_000);
        tracker
            .check_and_consume(MessagePriority::P0, 20_000)
            .expect("under the p0 share");
        // Remaining global room exists but the P0 share is spent: the seam
        // surfaces exhaustion so routing switches P0 elsewhere — nothing is
        // ever held waiting for budget.
        assert!(tracker
            .check_and_consume(MessagePriority::P0, 5_000)
            .is_err());
        assert!(tracker.duty_cycle_remaining_fraction() > 0.0);
        clock.store(1_000_000 + 3_600_000, Ordering::Relaxed);
        tracker
            .check_and_consume(MessagePriority::P0, 20_000)
            .expect("window rolled: p0 share restored");
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
        q.push_deferred(MessagePriority::P4, 7);
        assert_eq!(q.len(), MAX_BACKLOG_ENTRIES + 1);
        // Drain still works over the temporarily-over-cap queue.
        let mut drained = 0;
        while q.pop_highest().is_some() {
            drained += 1;
        }
        assert_eq!(drained, MAX_BACKLOG_ENTRIES + 1);
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
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        assert_eq!(t.state(), TransportState::Available);

        // Bulk share is 1800 ms; a full 255-B P4 frame (237-B payload +
        // 18-B header, RT-101) costs 1251 ms -> one fits.
        let peer = PeerId([2u8; 32]);
        t.send(&peer, &msg(MessagePriority::P4, 237))
            .await
            .expect("first bulk send fits the 5% share");
        let err = t
            .send(&peer, &msg(MessagePriority::P4, 237))
            .await
            .expect_err("bulk share spent");
        assert!(matches!(err, TransportError::Busy));
        assert_eq!(t.metrics_snapshot().duty_refusals, 1);
        assert!(t.duty_cycle_remaining() > 0.9, "only the bucket is spent");

        // Window rolls -> the same send succeeds again.
        _clock.store(5_000_000 + 3_600_000, Ordering::Relaxed);
        t.send(&peer, &msg(MessagePriority::P4, 237))
            .await
            .expect("rolling hour restored the budget");
        let m = t.metrics_snapshot();
        assert_eq!(m.packets_tx, 2);
        assert_eq!(m.airtime_consumed_ms, 2 * 1_251);
    }

    #[tokio::test]
    async fn duty_bills_full_encoded_frame_not_just_payload_rt101() {
        let (_clock, tracker) = fake_tracker(0);
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");

        // A 1-B payload rides a 19-B frame: airtime(SF9, PL=19) = 186 ms,
        // NOT the payload-only 104 ms. RT-101 regression anchor.
        let peer = PeerId([9u8; 32]);
        t.send(&peer, &msg(MessagePriority::P4, 1))
            .await
            .expect("minimal frame fits");
        assert_eq!(t.metrics_snapshot().airtime_consumed_ms, 186);

        // Flood bound: Bulk cap 1800 ms admits floor(1800/186) = 9 minimal
        // frames; the 10th must refuse BEFORE the true on-air time can
        // exceed the Table-I share (the pre-fix accounting admitted 17).
        let mut admitted = 0;
        for _ in 1..=17 {
            if t.send(&peer, &msg(MessagePriority::P4, 1)).await.is_ok() {
                admitted += 1;
            }
        }
        assert_eq!(admitted, 8, "9 total minimal frames max per bulk hour");
        let total = t.metrics_snapshot().airtime_consumed_ms;
        assert!(
            total <= 1_800,
            "bulk bucket accounting {total} ms must stay under its 1800 ms share"
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
        a.close().await.expect("close sim link");
        let peer = PeerId([10u8; 32]);
        let err = t
            .send(&peer, &msg(MessagePriority::P4, 237))
            .await
            .expect_err("closed link fails the tx");
        assert!(matches!(err, TransportError::Io(_)));
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
    async fn p0_send_rides_sf12_airtime_accounting() {
        let (_clock, tracker) = fake_tracker(0);
        let t = LoRaTransport::with_tracker("lora-0", Arc::new(tracker));
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        t.send(&PeerId([1u8; 32]), &msg(MessagePriority::P0, 237))
            .await
            .expect("p0 within its 60% share");
        // Full 255-B frame billed (RT-101): airtime(SF12, PL=255) = 9020 ms.
        assert_eq!(t.metrics_snapshot().airtime_consumed_ms, 9_020);
        assert_eq!(t.send_priority_hint(), MessagePriority::P4, "hint unset");
        t.set_send_priority_hint(MessagePriority::P0);
        assert_eq!(t.send_priority_hint(), MessagePriority::P0);
    }

    #[tokio::test]
    async fn oversize_payload_is_rejected_up_front() {
        let t = LoRaTransport::new("lora-0");
        let (a, _b) = sim_pair();
        t.attach_adapter(a).await.expect("adapter opens");
        let err = t
            .send(&PeerId([3u8; 32]), &msg(MessagePriority::P4, 256))
            .await
            .expect_err(">255 B cannot ride one LoRa frame");
        assert!(matches!(err, TransportError::Protocol(_)));
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

        // Spend most of the P2 (Location, 10% = 3600 ms) bucket: two full
        // 255-B frames at 1251 ms each leave only 1098 ms — less than
        // another full frame (RT-101 billing basis).
        t.send(&peer, &msg(MessagePriority::P2, 237))
            .await
            .expect("p2 frame #1");
        t.send(&peer, &msg(MessagePriority::P2, 237))
            .await
            .expect("p2 frame #2");
        assert!(
            t.send(&peer, &msg(MessagePriority::P2, 237)).await.is_err(),
            "third frame exceeds the location bucket share"
        );

        // Hold a message that cannot fit the remaining share (1098 ms left,
        // a full P2 frame needs 1251 ms).
        t.enqueue_backlog(msg(MessagePriority::P2, 237))
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
