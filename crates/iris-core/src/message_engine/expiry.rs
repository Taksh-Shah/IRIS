//! TTL expiry + clock-skew handling — MSG-001 (R1, R7).
//!
//! TTL is enforced at **three points** (per MSG_DESIGN.md §Acceptance #3):
//! insertion, dequeue, and relay/processing. Clock skew is handled per RFC 9171
//! lifetime semantics (R7): the raw creation time from the envelope is trusted
//! only within a skew budget; envelopes with a creation time that is
//! impossible (future beyond budget) are treated as skew-suspect and evaluated
//! against the **arrival-time** lifetime instead.

use std::time::Duration;

/// Default budget for how far a peer's clock may be ahead of ours before we
/// treat the creation timestamp as suspect (MSG_DESIGN.md default 300 s;
/// RES-0010 R7 allows widening to a per-peer 3600 s cap when peer-clock
/// calibration exists — `DEFAULT_SKEW_BUDGET_SECS` stays the conservative base).
pub const DEFAULT_SKEW_BUDGET_SECS: u64 = 300;

/// Widest skew a peer clock may ever have (RES-0010 R7; grounded on the
/// Android NTP polling interval of 18 h → real-world skew can reach ~1 h).
pub const MAX_SKEW_CAP_SECS: u64 = 3600;

/// Why a message was (or should be) expired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiryReason {
    /// Wall-clock expiry: `created_at + ttl <= now`.
    WallClock,
    /// Creation time is in the future beyond the skew budget → lifetime judged
    /// from arrival time (RFC 9171 §4.4.2). Prevents a forged/ahead clock from
    /// granting an effectively infinite lifetime.
    ClockSkewSuspect,
}

/// Absolute expiry instant for an envelope.
pub fn expires_at(created_at_unix: u64, ttl_seconds: u64) -> u64 {
    created_at_unix.saturating_add(ttl_seconds)
}

/// Compute the expiry wall-clock instant given the envelope creation time and
/// the engine's current time.
///
/// Returns `None` if the creation time is suspect (future beyond skew budget) —
/// callers should then use the arrival-time lifetime (`now + ttl`).
pub fn compute_expiry(created_at_unix: u64, ttl_seconds: u64, now_unix: u64) -> Option<u64> {
    if created_at_unix > now_unix.saturating_add(DEFAULT_SKEW_BUDGET_SECS) {
        // Created in the future beyond the skew budget: R7 — do not grant a
        // forged clock extra lifetime. Fall back to arrival-time lifetime.
        return None;
    }
    Some(expires_at(created_at_unix, ttl_seconds))
}

/// Wall-clock TTL check. `now_unix` is the local clock.
///
/// Skew-suspect messages (creation time beyond the future skew budget) have no
/// usable wall-clock basis and are treated as EXPIRED here.
///
/// The previous behaviour returned `false` for them, intending an "arrival-time
/// lifetime" of `now + ttl`. But no arrival time was ever recorded, so the
/// comparison was re-evaluated against a moving `now` on every call and could
/// never become true: a single envelope with `timestamp = now + 10^9` was
/// immortal at every checkpoint — insert, receive, dequeue and the GC sweep —
/// on every node that saw it. That is a trivial, permanent memory-exhaustion
/// vector, so this now fails closed.
///
/// Prefer [`is_expired_from_arrival`] wherever the caller knows when the
/// message actually arrived; it keeps skew-suspect messages alive for a real,
/// bounded TTL instead of dropping them, which matters for a device whose RTC
/// is genuinely wrong rather than forged.  Always use it at ingress
/// (`process_incoming`) where the arrival time equals `now_unix`.
pub fn is_expired(created_at_unix: u64, ttl_seconds: u64, now_unix: u64) -> bool {
    match compute_expiry(created_at_unix, ttl_seconds, now_unix) {
        Some(expiry) => expiry <= now_unix,
        None => true,
    }
}

/// TTL check that falls back to a recorded arrival instant when the sender's
/// clock is not trustworthy.
///
/// `arrived_at_unix` must be a fixed instant captured when the message was
/// first seen locally — not `now`. Because it does not move, the skew-suspect
/// branch genuinely expires.
pub fn is_expired_from_arrival(
    created_at_unix: u64,
    ttl_seconds: u64,
    arrived_at_unix: u64,
    now_unix: u64,
) -> bool {
    match compute_expiry(created_at_unix, ttl_seconds, now_unix) {
        Some(expiry) => expiry <= now_unix,
        None => arrived_at_unix.saturating_add(ttl_seconds) <= now_unix,
    }
}

/// Whether a creation timestamp is skew-suspect (future beyond the budget).
pub fn is_skew_suspect(created_at_unix: u64, now_unix: u64) -> bool {
    created_at_unix > now_unix.saturating_add(DEFAULT_SKEW_BUDGET_SECS)
}

/// Reason-based expiry decision used by the engine when it decides to expire a
/// message (also recorded in observability).
///
/// Skew-suspect messages (creation time beyond the future skew budget) cannot
/// have a wall-clock expiry basis — their lifetime is judged from the recorded
/// arrival instant instead.  When a suspect message is being dropped (e.g. on
/// the retry path where no arrival instant is available) this returns
/// `Some(ExpiryReason::ClockSkewSuspect)` so the reason is observable.
/// Consistent with `is_expired` failing closed for suspect clocks.
pub fn expiry_reason(
    created_at_unix: u64,
    ttl_seconds: u64,
    now_unix: u64,
) -> Option<ExpiryReason> {
    match compute_expiry(created_at_unix, ttl_seconds, now_unix) {
        Some(expiry) if expiry <= now_unix => Some(ExpiryReason::WallClock),
        None => Some(ExpiryReason::ClockSkewSuspect),
        _ => None,
    }
}

/// Remaining lifetime as a `Duration` from now, used by the fragmenter to cap
/// its reassembly deadline (R8: deadline = min(30 s, TTL remainder, priority cap)).
pub fn remaining_lifetime(created_at_unix: u64, ttl_seconds: u64, now_unix: u64) -> Duration {
    let expiry = compute_expiry(created_at_unix, ttl_seconds, now_unix)
        .unwrap_or_else(|| now_unix.saturating_add(ttl_seconds));
    let remain = expiry.saturating_sub(now_unix);
    Duration::from_secs(remain)
}

/// TTL check at enqueue time. Returns `false` if the message is already
/// expired and must not enter the queue.
pub fn check_ttl_before_insert(created_at_unix: u64, ttl_seconds: u64, now_unix: u64) -> bool {
    !is_expired(created_at_unix, ttl_seconds, now_unix)
}

/// A queue entry that can be expired. Used by the GC task.
#[derive(Debug, Clone, Copy)]
pub struct TtlCheck {
    pub created_at_unix: u64,
    pub ttl_seconds: u64,
}

impl TtlCheck {
    pub fn expired(&self, now_unix: u64) -> bool {
        is_expired(self.created_at_unix, self.ttl_seconds, now_unix)
    }
}

/// Current Unix time in seconds (wall clock; used for TTL decisions).
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_752_000_000;

    #[test]
    fn ttl_check_at_boundary() {
        // Created exactly `ttl` ago → expired (expiry <= now).
        assert!(is_expired(NOW - 300, 300, NOW));
        assert!(!is_expired(NOW - 299, 300, NOW));
        assert!(!is_expired(NOW, 300, NOW));
    }

    #[test]
    fn ttl_rejects_expired_at_insert() {
        assert!(!check_ttl_before_insert(NOW - 301, 300, NOW));
        assert!(check_ttl_before_insert(NOW - 100, 300, NOW));
    }

    #[test]
    fn skew_suspect_clock_does_not_grant_infinite_lifetime() {
        // Clock 10 000 s ahead (>> 300 s budget) → compute_expiry returns None.
        assert_eq!(compute_expiry(NOW + 10_000, 300, NOW), None);
        // ... and the arrival-time fallback keeps it alive for ttl, not forever.
        let fallback = remaining_lifetime(NOW + 10_000, 300, NOW);
        assert!(fallback <= Duration::from_secs(301), "must not exceed ~ttl");
    }

    #[test]
    fn skew_suspect_message_actually_expires_on_the_drop_path() {
        // Regression: `is_expired` is what every checkpoint (insert, receive,
        // dequeue, GC) actually calls, and it used to return false for a
        // skew-suspect timestamp on EVERY call, re-evaluated against a moving
        // `now`. One envelope with a far-future timestamp was therefore
        // immortal on every node that saw it.
        //
        // The sibling test above passes either way because it exercises
        // `remaining_lifetime`, which is not on the drop path.
        assert!(
            is_expired(NOW + 1_000_000_000, 300, NOW),
            "forged-future timestamp must not survive the TTL check"
        );
        assert!(
            is_expired(NOW + 1_000_000_000, 300, NOW + 10_000_000),
            "and must still be expired however far the clock advances"
        );
    }

    #[test]
    fn arrival_based_expiry_bounds_a_skewed_sender() {
        // A device with a genuinely wrong RTC should still get its full TTL,
        // measured from when we saw the message — and then expire.
        let created = NOW + 1_000_000; // skew-suspect
        let arrived = NOW;
        assert!(
            !is_expired_from_arrival(created, 300, arrived, NOW + 100),
            "within ttl of arrival"
        );
        assert!(
            is_expired_from_arrival(created, 300, arrived, NOW + 301),
            "past ttl of arrival"
        );
    }

    #[test]
    fn within_skew_budget_is_trusted() {
        // 250 s ahead is within the 300 s budget → trusted, not suspect.
        assert!(!is_skew_suspect(NOW + 250, NOW));
        assert!(
            is_skew_suspect(NOW + 10_000, NOW),
            "beyond budget is suspect"
        );
        // Trusted clock + short TTL: expires 10 s after its (future) creation.
        assert!(
            is_expired(NOW + 250, 10, NOW + 261),
            "short TTL still expires"
        );
        assert!(!is_expired(NOW + 250, 10, NOW + 100), "not yet expired");
    }

    #[test]
    fn reason_reports_wall_clock_and_skew_suspect() {
        assert_eq!(
            expiry_reason(NOW - 100, 50, NOW),
            Some(ExpiryReason::WallClock)
        );
        // Skew-suspect messages (created far beyond budget): compute_expiry
        // returns None → expiry_reason returns ClockSkewSuspect so the drop
        // reason is observable in logs (consistent with is_expired failing closed).
        assert_eq!(
            expiry_reason(NOW + 20_000, 1, NOW + 10_001),
            Some(ExpiryReason::ClockSkewSuspect)
        );
        // Not yet expired (within TTL and within skew budget) → None.
        assert_eq!(expiry_reason(NOW, 300, NOW), None);
    }
}
