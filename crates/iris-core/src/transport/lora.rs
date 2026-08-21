//! LoRa long-range transport — LORA-001.
//!
//! Implements the LoRa gateway transport as an **adapter-injected** node (the
//! BLE-001/BLE-002 `BleAdapter`-seam lesson): a platform-neutral Rust core plus
//! a `LoRaLinkAdapter` trait whose implementations talk to the external LoRa
//! module over AT-serial (EBYTE E22-900M30S / USB-CDC) or SPI-native (Waveshare
//! SX1262 HAT on RPi) firmware. The physical SX1262/E22 firmware + GATT-bridge
//! packaging is hardware-gated (BLK-0005 / GAP-004 / RES-0027 G-2).
//!
//! Regulatory floor = **WPC G.S.R. 853(E) 2021 Table-I**: 865–868 MHz,
//! **≤25 mW e.r.p. (≈14 dBm)**, **≤1% duty** (36 s / device / hour), applied to
//! the **whole transmission** (airtime, not payload bytes). `ComplianceConfig`
//! pins the field config; the `DutyCycleTracker` is a hard legal floor with
//! **no runtime override** (Meshtastic `override_duty_cycle_limit` deliberately
//! NOT followed). P0 switches to the next-best transport on exhaustion — never
//! held, never overrides the budget. LoRaWAN/IN865 (30 dBm EIRP) is a
//! network-planning figure for a future alternate uplink only.
//!
//! v1 = **raw-LoRa P2P** (no LoRaWAN stack, keys, or join ceremony). Default
//! radio profile **SF9 / BW125 / CR4/5**; **P0 uses SF12** (−129/−136 dBm
//! sensitivity, AN1200.13 → ≈+7 dB link budget). No crypto changes: the
//! envelope X25519+ChaCha20-Poly1305 (CRYPTO-001) is transported unchanged.
//!
//! All simulated / link-budget evidence is **SIMULATION_VALIDATED** only
//! (BLK-0005 / GAP-004 / RES-0027 G-2/G-3): no field claims.

use std::pin::Pin;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures_util::stream::Stream;

use crate::message::{
    DiscoveryConfig, IncomingMessage, MessagePriority, NodeAdvertisement, PeerId, PeerInfo,
    SendReceipt, SerializedMessage, TransportLink,
};
use crate::transport::{
    AtomicState, Transport, TransportCapabilities, TransportCost, TransportCostClass, TransportId,
    TransportState, TransportStateEvent,
};
use crate::TransportError;

// ==== 1. Regulatory compliance (D-7) ====

/// Static regulatory rule set consumed by the [`DutyCycleTracker`].
///
/// Pins the field config per WPC G.225.S R. 853(E) 2021 Table-I (L1 primary
/// source: thc.nic.in/G25977.pdf): 866.0 MHz, ≤25 mW e.r.p. (≈14 dBm), ≤1% duty
/// over a rolling 1-h window (36,000 ms per device-hour). Airtime counts the
/// whole packet on-air time at SF/BW/CR — not payload bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ComplianceConfig {
    pub freq_hz: u64,
    pub max_erp_dbm: i8,
    pub duty_percent: f32,
    pub window_ms: u64,
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

/// Priority bucket used for proportional airtime allocation (60/25/10/5).
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

    /// Air-time share under contention (REQ-ROUTE-C-002: 60/25/10/5).
    pub fn budget_fraction(self) -> f32 {
        match self {
            DutyBucket::Emergency => 0.60,
            DutyBucket::Medical => 0.25,
            DutyBucket::Location => 0.10,
            DutyBucket::Bulk => 0.05,
        }
    }
}

/// The next legal send time produced by [`DutyCycleTracker::check_and_consume`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NextWindow {
    pub at: Instant,
    pub remaining_fraction: f32,
}

/// Why a LoRa send was refused: the 1% duty budget (or this bucket's share) is
/// exhausted this window. Carries the next legal send time so the routing layer
/// can schedule around it (RFC 9171 §6.9 analogy, RES-0008 R7) — never a panic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BudgetExhausted {
    pub at: Instant,
    pub remaining_fraction: f32,
}

/// A sliding 1-h window, 36,000 ms airtime budget, global per-device (all
/// 865–868 MHz TX), **no override** (DEC-LORA-0002).
///
/// Under contention each priority bucket is capped to its proportional share
/// (P0 60% / P1 25% / P2 10% / P3+ 5% of the budget). Both a bucket cap and the
/// global cap are enforced.
#[derive(Debug)]
pub struct DutyCycleTracker {
    cfg: ComplianceConfig,
    /// (tx_start: Instant, airtime_ms, bucket) — oldest at the front.
    tx: std::sync::Mutex<std::collections::VecDeque<(Instant, u64, DutyBucket)>>,
}

impl DutyCycleTracker {
    pub fn new(cfg: ComplianceConfig) -> Self {
        DutyCycleTracker {
            cfg,
            tx: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }

    fn now_ms() -> u64 {
        (Instant::now().duration_since(std::time::UNIX_EPOCH)).to_millis()
    }

    /// Drop entries older than one window; return (global_used_ms, bucket_usage).
    fn prune(&self, now_ms: u64) -> (u64, std::collections::HashMap<DutyBucket, u64>) {
        let q = self.tx.lock().unwrap();
        while !q.is_front().is_empty() && now_ms - q.front().0 > self.cfg.window_ms {
            let _ = q.pop_front();
        }
        let mut used = 0u64;
        let mut by_bucket = std::collections::HashMap::new();
        for (t, air, b) in q {
            if now_ms - t <= self.cfg.window_ms {
                used += air;
                by_bucket[b] = (by_bucket[b]? 0 else 0) + air;
            }
        }
        (used, by_bucket)
    }

    /// Fraction of the per-device hourly budget still unused in [0,1].
    pub fn duty_cycle_remaining_fraction(&self) -> f32 {
        let (used, _) = self.prune(Self::now_ms());
        let frac = 1.0 - (used as f32) / (self.cfg.airtime_budget_ms as f32);
        if frac < 0.0 { 0.0 } else { frac }
    }

    /// How long until the global budget next frees (ms), given usage.
    fn next_free_delay_ms(&self, now_ms: u64, used: u64) -> u64 {
        if used >= self.cfg.airtime_budget_ms {
            // Earliest entry drops out at window boundary.
            let mut earliest: Option<u64> = None;
            for (t, _, _) in self.tx.lock().unwrap() {
                let age = now_ms - t;
                if earliest.is_none() || age < earliest.unwrap() {
                    earliest = Some(age);
                }
            }
            if earliest.is_some() {
                // +1 ms grace past the window edge.
                (self.cfg.window_ms - earliest.unwrap()) + 1
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Reserve `airtime_ms` for `class`, if the global and bucket budgets allow.
    pub fn check_and_consume(
        &self,
        class: MessagePriority,
        airtime_ms: u64,
    ) -> Result<NextWindow, BudgetExhausted> {
        let now = Self::now_ms();
        let (used, by_bucket) = self.prune(now);
        let bucket = DutyBucket::from_priority(class);
        let bucket_cap = (self.cfg.airtime_budget_ms as f32 * bucket.budget_fraction()) as u64;
        let bucket_used = by_bucket[bucket]? 0 else 0;
        let remaining_global = self.cfg.airtime_budget_ms - used;
        let remaining_bucket = if bucket_cap > bucket_used { bucket_cap - bucket_used } else { 0 };

        if airtime_ms > remaining_global || airtime_ms > remaining_bucket {
            let delay = self.next_budget_recovery_ms(now, used, bucket_cap, bucket_used);
            let at = Instant::UNIX_EPOCH + Duration::from_millis(now + delay);
            Err(DutyBudgetExhausted {
                at,
                remaining_fraction: self.duty_cycle_remaining_fraction(),
            })
        } else {
            self.tx.lock().unwrap().push_back((
                now as Instant::EpochMilliseconds,
                airtime_ms,
                bucket,
            ));
            Ok(NextWindow {
                at: Instant::UNIX_EPOCH + Duration::from_millis(now + self.locked_wait_before_next(now, airtime_ms)),
                remaining_fraction: self.duty_cycle_remaining_fraction(),
            })
        }
    }

    /// Delay until the global budget (or this bucket's cap) frees, or 0 if a
    /// slot if free.
    fn next_budget_delay_ms(&self, now: u64, used: u64, cap: u64, bucket_used: u64) -> u64 {
        // If the global budget is free, the constraint is the bucket cap.
        let global_blocked = used >= self.cfg.airtime_budget_ms;
        if !global_blocked {
            // Bucket cap: recovery ≈1% per 100 s of silence is a routing-layer
            // property; here the cap frees when the oldest bucket entry ages out.
            return 0;
        }
        self.next_budget_delay_ms_internal(now, used)
    }