//! Priority outbound queue — MSG-001 (R3: strict priority + fairness).
//!
//! Strict-priority ordering: P0 always dequeues first, FIFO within a priority,
//! higher delivery-probability first as a tiebreak (MSG_DESIGN.md §Priority
//! Queue). BinaryHeap (max-heap) with reversed `Ord` so P0 sits on top.
//!
//! **Starvation prevention (RES-0010 R3):** a DRR-style fairness gate replaces
//! the naive "insert one lower-priority after 60 s" heuristic. The queue tracks
//! how many consecutive P0 messages were dispatched; after
//! [`DEFAULT_P0_BUDGET`] consecutive P0 dispatches with no lower-priority
//! dispatch since the last gate reset, one P0–P7 message (highest priority
//! available below P0) is force-dequeued and the gate resets. Every non-empty
//! class therefore receives airtime during sustained emergency traffic.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::time::Instant;

use crate::message::MessagePriority;
use crate::protocol::Envelope;

/// Number of consecutive P0 dispatches before the fairness gate admits one
/// lower-priority message (MSG_DESIGN.md: "after 60 seconds of continuous P0").
pub const DEFAULT_P0_BUDGET: u64 = 60;

/// A message waiting in the outbound queue.
#[derive(Debug, Clone)]
pub struct QueuedMessage {
    /// Wire envelope (payload + routing hints).
    pub envelope: Envelope,
    /// Local enqueue time — FIFO within priority.
    pub created_at: Instant,
    /// Delivery probability hint for tie-breaking (routing-hints
    /// `delivery_prob`, default 0.5).
    pub delivery_prob: f64,
    /// Attempt count (for backoff/retry bookkeeping).
    pub attempts: u32,
    /// Next retry deadline (`None` = due immediately).
    pub next_retry: Option<Instant>,
    /// Whether this message originated locally (true) or is being relayed.
    pub local: bool,
    /// Node-local scheduling priority. Defaults to `envelope.priority` but may
    /// differ when the emergency gate downgrades a SOS beyond its P0 allowance
    /// (PM-1): the wire envelope keeps its original, verifiable priority while
    /// this field drives queue ordering, so the relay carries a byte-identical
    /// and verifiable envelope.
    pub scheduling_priority: MessagePriority,
}

impl QueuedMessage {
    /// Wrap an envelope with engine defaults.
    pub fn new(envelope: Envelope) -> Self {
        let delivery_prob = envelope
            .routing_hints
            .as_ref()
            .and_then(|h| h.delivery_prob)
            .unwrap_or(0.5);
        let scheduling_priority = envelope.priority;
        QueuedMessage {
            envelope,
            created_at: Instant::now(),
            delivery_prob,
            attempts: 0,
            next_retry: None,
            local: true,
            scheduling_priority,
        }
    }

    /// Override the scheduling priority without touching the wire envelope.
    pub fn with_scheduling_priority(mut self, p: MessagePriority) -> Self {
        self.scheduling_priority = p;
        self
    }

    pub fn priority(&self) -> MessagePriority {
        self.scheduling_priority
    }
}

// BinaryHeap is a max-heap. We want the *highest* priority (lowest P-number)
// to be the "greatest" element.
impl PartialEq for QueuedMessage {
    fn eq(&self, other: &Self) -> bool {
        self.priority() == other.priority()
            && self.created_at == other.created_at
            && self.delivery_prob.total_cmp(&other.delivery_prob) == Ordering::Equal
    }
}
impl Eq for QueuedMessage {}

impl PartialOrd for QueuedMessage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedMessage {
    fn cmp(&self, other: &Self) -> Ordering {
        // P0 (0) sorts as the greatest element: reverse priority order.
        other
            .priority()
            .cmp(&self.priority())
            // FIFO within priority: older first.
            .then_with(|| other.created_at.cmp(&self.created_at))
            // Higher delivery probability first.
            .then_with(|| self.delivery_prob.total_cmp(&other.delivery_prob))
    }
}

/// Priority queue with a DRR-style fairness gate.
#[derive(Debug)]
pub struct PriorityQueue {
    heap: BinaryHeap<QueuedMessage>,
    /// P0 dispatches since the last fairness-gate reset.
    p0_dispatched_since_gate: u64,
    /// Lower-priority dispatches since the last fairness-gate reset.
    lower_dispatched_since_gate: u64,
    /// Consecutive-P0 budget before admitting a lower-priority message.
    p0_budget: u64,
}

impl Default for PriorityQueue {
    fn default() -> Self {
        Self::new(DEFAULT_P0_BUDGET)
    }
}

impl PriorityQueue {
    pub fn new(p0_budget: u64) -> Self {
        PriorityQueue {
            heap: BinaryHeap::new(),
            p0_dispatched_since_gate: 0,
            lower_dispatched_since_gate: 0,
            p0_budget: p0_budget.max(1),
        }
    }

    pub fn push(&mut self, item: QueuedMessage) {
        self.heap.push(item);
    }

    pub fn len(&self) -> usize {
        self.heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn peek(&self) -> Option<&QueuedMessage> {
        self.heap.peek()
    }

    pub fn iter(&self) -> impl Iterator<Item = &QueuedMessage> {
        self.heap.iter()
    }

    /// Pop the next message to dispatch.
    ///
    /// Implements the fairness gate (R3): while a P0 is on top, it is normally
    /// dispatched; once `p0_budget` consecutive P0 dispatches have occurred
    /// since the last lower-priority dispatch, one lower-priority message is
    /// force-dequeued (highest priority below P0 first).
    ///
    /// HW-3: `next_retry` (the backoff deadline `requeue_or_fail` computes
    /// from `RetryPolicy::timeout_for_attempt`) used to be entirely
    /// decorative — this method popped the heap's top unconditionally,
    /// regardless of whether that item's backoff had actually elapsed. The
    /// delivery loop polls every 50ms (`poll_interval`), so a message whose
    /// first send attempt failed was retried on the very next 50ms tick
    /// instead of waiting out its intended backoff — burning through the
    /// whole `max_attempts` budget (as low as 2, for P4-P7 bulk) in a
    /// fraction of a second. Confirmed live: a message created at
    /// `msg.created` reached `msg.delivery_failed` ("delivery attempt budget
    /// exhausted") ~144ms later, despite a live BLE link having been
    /// confirmed (`discovery.connect_ok`) five seconds earlier — there was
    /// never a real chance for the transport to become selectable again
    /// before the attempt budget ran out.
    ///
    /// Fix, first attempt: if the top of the heap wasn't due yet, return
    /// `None` for this tick rather than force-dequeuing it. Proven
    /// insufficient in practice within the same session: a message whose
    /// first attempt fails gets requeued with a long backoff (P4's initial
    /// timeout is 600s) but keeps its ORIGINAL `created_at` — so it still
    /// sorts ahead of every later P4 message in the FIFO tiebreak. That
    /// blocked-but-undue item then sat at the top of the heap and silently
    /// starved every message behind it for the rest of its backoff window,
    /// confirmed live: a second message sent immediately after the first
    /// one failed produced no `deliver_outbound` trace at all for 26+
    /// seconds, though the connection was confirmed live throughout.
    ///
    /// Real fix: when the top isn't due, scan for the best DUE item instead
    /// of giving up. `BinaryHeap` has no cheap "remove this specific
    /// element" — drain-filter-rebuild is O(n), which is fine at this
    /// queue's realistic depth (a mesh node's outbound backlog, not a
    /// datacenter queue) and is the same pattern the fairness-gate branch
    /// below already uses for the same reason.
    pub fn dequeue(&mut self, now: Instant) -> Option<QueuedMessage> {
        if self.heap.is_empty() {
            return None;
        }
        let top_due = self
            .heap
            .peek()
            .map(|top| top.next_retry.map(|d| now >= d).unwrap_or(true))
            .unwrap_or(false);
        if !top_due {
            let all: Vec<QueuedMessage> = self.heap.drain().collect();
            let (due, not_due): (Vec<_>, Vec<_>) = all.into_iter().partition(|m| {
                m.next_retry.map(|d| now >= d).unwrap_or(true)
            });
            self.heap.extend(not_due);
            let Some(best) = due.into_iter().max() else {
                return None;
            };
            // `best` bypasses the fairness-gate bookkeeping below (it was
            // never a normal top-of-heap pop) — deliberately: the gate
            // exists to bound cross-priority starvation during a SUSTAINED
            // P0 stream, not to arbitrate the rare case of digging past a
            // backed-off item. Not updating the gate counters here is
            // conservative (it can only delay a fairness admission, never
            // skip one) and avoids the gate's own bookkeeping disagreeing
            // with which priority was "really" dispatched this tick.
            return Some(best);
        }

        let budget_hit = self.p0_dispatched_since_gate >= self.p0_budget
            && self.lower_dispatched_since_gate == 0;

        if budget_hit {
            // Fairness: admit one message strictly below P0 (highest priority
            // available below P0, FIFO within it).
            let all: Vec<QueuedMessage> = self.heap.drain().collect();
            let mut p0 = Vec::new();
            let mut low = Vec::new();
            for m in all {
                if m.priority() == MessagePriority::P0 {
                    p0.push(m);
                } else {
                    low.push(m);
                }
            }
            self.heap.extend(p0);
            let chosen = low
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| {
                    b.priority()
                        .cmp(&a.priority())
                        .then_with(|| a.created_at.cmp(&b.created_at))
                })
                .map(|(i, _)| i);
            if let Some(i) = chosen {
                let selected = low.remove(i);
                self.heap.extend(low);
                self.lower_dispatched_since_gate += 1;
                self.p0_dispatched_since_gate = 0;
                return Some(selected);
            }
        }

        let top = self.heap.pop().expect("checked non-empty");
        if top.priority() == MessagePriority::P0 {
            self.p0_dispatched_since_gate += 1;
            self.lower_dispatched_since_gate = 0;
            // "Consecutive" is the whole point of the gate: the lower counter
            // has to clear when a P0 run starts, otherwise it only ever climbs.
            // Leaving it set made `budget_hit` (which requires == 0) unsatisfiable
            // after the very first lower-priority dispatch, so the gate fired
            // once per process and a sustained P0 stream starved P1-P7 forever.
        } else {
            self.lower_dispatched_since_gate += 1;
            self.p0_dispatched_since_gate = 0;
        }
        Some(top)
    }

    /// Remove the first message matching `id` (rollback on persist failure).
    /// Returns true if a message was removed.
    pub fn remove_by_id(&mut self, id: &crate::protocol::MessageId) -> bool {
        let all: Vec<QueuedMessage> = self.heap.drain().collect();
        let (mut keep, mut removed) = (Vec::new(), None);
        for m in all {
            if removed.is_none() && m.envelope.message_id == *id {
                removed = Some(m);
            } else {
                keep.push(m);
            }
        }
        self.heap.extend(keep);
        removed.is_some()
    }

    /// Expire messages whose TTL elapsed. Returns expired entries (their
    /// envelopes) so the caller can transition them to `Expired`.
    pub fn drain_expired<F>(&mut self, mut expired: F) -> Vec<Envelope>
    where
        F: FnMut(&Envelope) -> bool,
    {
        let (live, dead): (Vec<_>, Vec<_>) = self.heap.drain().partition(|m| !expired(&m.envelope));
        self.heap.extend(live);
        dead.into_iter().map(|m| m.envelope).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::MessageId;

    fn env(priority: MessagePriority, seq: u8) -> Envelope {
        let mut m = Envelope {
            version: 1,
            message_id: MessageId::from_bytes([seq; 16]),
            sender_id: vec![1],
            recipient_id: vec![2],
            priority,
            ttl_seconds: 3600,
            timestamp: 1_752_000_000,
            hop_count: 0,
            max_hops: None,
            payload_type: crate::protocol::ContentType::Sos,
            payload_size: 0,
            payload_hash: [0; 32],
            payload: Vec::new(),
            payload_ref: None,
            signature: None,
            encryption_hdr: None,
            routing_hints: None,
            auth_cert_chain: None,
        };
        m.message_id = MessageId::from_bytes([seq; 16]);
        m
    }

    #[test]
    fn hw4_dequeue_scans_past_a_blocked_top_for_a_due_item() {
        // HW-4 (second half): the first fix for this made dequeue() return
        // None whenever the TOP of the heap wasn't due — but a requeued
        // message keeps its ORIGINAL created_at, so a failed P4 send (600s
        // initial backoff) still sorts ahead of every later P4 message in
        // the FIFO tiebreak. That not-yet-due item then silently starved
        // everything behind it for its whole backoff window. Confirmed
        // live: a second message sent right after the first one failed
        // produced no delivery-loop trace at all for 26+ seconds, despite a
        // confirmed-live connection throughout.
        let mut q = PriorityQueue::new(60);
        let now = Instant::now();

        let mut blocked = QueuedMessage::new(env(MessagePriority::P4, 1));
        blocked.next_retry = Some(now + std::time::Duration::from_secs(600));
        q.push(blocked);

        // A later, same-priority message that IS due — must not be starved
        // by the blocked one sitting ahead of it in FIFO order.
        let due = QueuedMessage::new(env(MessagePriority::P4, 2));
        let due_id = due.envelope.message_id;
        q.push(due);

        let got = q.dequeue(now).expect("the due message must be found past the blocked top");
        assert_eq!(got.envelope.message_id, due_id);
        assert_eq!(q.len(), 1, "the blocked message must stay queued, not be dropped or returned");
    }

    #[test]
    fn hw3_dequeue_respects_next_retry_backoff() {
        // HW-3: dequeue() used to pop the heap's top unconditionally,
        // ignoring `next_retry` entirely — a requeued message was retried on
        // the very next poll_interval tick instead of waiting out its
        // computed backoff, burning through max_attempts (as low as 2 for
        // bulk priorities) in a fraction of a second. Confirmed live: a
        // message went from `msg.created` to `msg.delivery_failed`
        // ("delivery attempt budget exhausted") in ~144ms despite a BLE link
        // having been confirmed live five seconds earlier.
        let mut q = PriorityQueue::new(60);
        let mut msg = QueuedMessage::new(env(MessagePriority::P4, 1));
        let now = Instant::now();
        msg.next_retry = Some(now + std::time::Duration::from_secs(30));
        q.push(msg);

        assert!(
            q.dequeue(now).is_none(),
            "a message whose backoff hasn't elapsed must not be returned"
        );
        assert_eq!(q.len(), 1, "the not-yet-due message must stay in the queue, not be dropped");

        assert!(
            q.dequeue(now + std::time::Duration::from_secs(31)).is_some(),
            "once the backoff deadline has passed, the message must dequeue normally"
        );
    }

    #[test]
    fn p0_always_on_top() {
        let mut q = PriorityQueue::new(60);
        q.push(QueuedMessage::new(env(MessagePriority::P4, 1)));
        q.push(QueuedMessage::new(env(MessagePriority::P0, 2)));
        q.push(QueuedMessage::new(env(MessagePriority::P1, 3)));
        assert_eq!(q.dequeue(Instant::now()).unwrap().envelope.priority, MessagePriority::P0);
        assert_eq!(q.dequeue(Instant::now()).unwrap().envelope.priority, MessagePriority::P1);
        assert_eq!(q.dequeue(Instant::now()).unwrap().envelope.priority, MessagePriority::P4);
        assert!(q.dequeue(Instant::now()).is_none());
    }

    #[test]
    fn fifo_within_priority() {
        let mut q = PriorityQueue::new(60);
        // Enqueue three P2s in order.
        for seq in [1u8, 2, 3] {
            let mut e = env(MessagePriority::P2, seq);
            // differentiate id
            e.message_id = MessageId::from_bytes([seq; 16]);
            q.push(QueuedMessage::new(e));
        }
        let ids: Vec<u8> = (0..3)
            .map(|_| q.dequeue(Instant::now()).unwrap().envelope.message_id.0[0])
            .collect();
        assert_eq!(ids, vec![1, 2, 3], "FIFO within priority");
    }

    #[test]
    fn fairness_gate_admits_lower_priority_after_budget() {
        let mut q = PriorityQueue::new(3); // small budget for the test
                                           // Continuous P0 stream + one P5 at the bottom.
        for seq in 1u8..=5 {
            let mut e = env(MessagePriority::P0, seq);
            e.message_id = MessageId::from_bytes([seq; 16]);
            q.push(QueuedMessage::new(e));
        }
        q.push(QueuedMessage::new(env(MessagePriority::P5, 9)));

        // Budget = 3 → pops: P0, P0, P0, then P5 (fairness), then P0s.
        let mut popped = Vec::new();
        while let Some(m) = q.dequeue(Instant::now()) {
            popped.push(m.envelope.priority);
        }
        assert_eq!(
            popped,
            vec![
                MessagePriority::P0,
                MessagePriority::P0,
                MessagePriority::P0,
                MessagePriority::P5,
                MessagePriority::P0,
                MessagePriority::P0,
            ],
            "P5 must be admitted after the P0 budget without starving"
        );
    }

    #[test]
    fn fairness_gate_keeps_firing_under_a_sustained_p0_stream() {
        // Regression: the gate requires `lower_dispatched_since_gate == 0`, and
        // that counter was incremented but never reset. It therefore fired at
        // most once per process — after the first lower-priority dispatch a
        // sustained P0 stream starved P1-P7 permanently.
        //
        // The sibling test above cannot catch this: it enqueues a single P5, so
        // the gate only ever needs to fire once.
        let mut q = PriorityQueue::new(2);
        for seq in 1u8..=8 {
            let mut e = env(MessagePriority::P0, seq);
            e.message_id = MessageId::from_bytes([seq; 16]);
            q.push(QueuedMessage::new(e));
        }
        for seq in 20u8..=22 {
            let mut e = env(MessagePriority::P5, seq);
            e.message_id = MessageId::from_bytes([seq; 16]);
            q.push(QueuedMessage::new(e));
        }

        let mut popped = Vec::new();
        while let Some(m) = q.dequeue(Instant::now()) {
            popped.push(m.envelope.priority);
        }

        let lower = popped.iter().filter(|p| **p == MessagePriority::P5).count();
        assert_eq!(lower, 3, "every queued P5 must eventually be dispatched");

        // With budget 2, a P5 must appear at least every third slot until the
        // lower-priority backlog drains — not just once.
        let first = popped
            .iter()
            .position(|p| *p == MessagePriority::P5)
            .expect("a P5 is dispatched");
        let second = popped
            .iter()
            .skip(first + 1)
            .position(|p| *p == MessagePriority::P5)
            .expect("the gate fires more than once");
        assert!(
            second <= 2,
            "gate must re-arm: second P5 came {} slots after the first",
            second + 1
        );
    }

    #[test]
    fn drain_expired_removes_only_dead() {
        let mut q = PriorityQueue::new(60);
        q.push(QueuedMessage::new(env(MessagePriority::P0, 1)));
        q.push(QueuedMessage::new(env(MessagePriority::P2, 2)));
        let dead: Vec<Envelope> = q.drain_expired(|e| e.priority != MessagePriority::P0);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].priority, MessagePriority::P2);
        assert_eq!(q.len(), 1);
        assert_eq!(q.peek().unwrap().envelope.priority, MessagePriority::P0);
    }
}
