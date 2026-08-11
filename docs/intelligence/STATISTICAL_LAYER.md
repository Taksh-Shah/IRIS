# L2 Statistical Prediction Layer

## Overview

The statistical layer learns from historical contact patterns to predict future routing
outcomes. It answers questions like: "If I give this message to relay node R, what is
the probability it will be delivered to destination D within the next hour?" These
predictions improve routing decisions beyond the deterministic hop-count minimization of L0.

## Prerequisite

L2 requires at least 7 days of contact history before predictions become meaningful.
A freshly deployed node uses L0 defaults until history is established.

```rust
pub fn has_sufficient_history(store: &ContactHistoryStore, min_days: u32) -> bool {
    store.oldest_record_age_days() >= min_days as i64
}
```

## Contact Probability

### Definition

P(a,b,T) = probability that node a comes into contact with node b within the next T minutes.

"Contact" is defined as a successful BLE or Wi-Fi Direct connection (peer is within range
and message exchange is possible). It does NOT require actual message exchange.

### Computation

```rust
pub fn contact_probability(
    self_id: &NodeId,
    target_id: &NodeId,
    window_minutes: u32,
    history: &ContactHistoryStore,
) -> Probability {
    let records = history.get_contacts_between(self_id, target_id, Duration::days(7));
    
    if records.is_empty() {
        return PROB_ZERO;  // No history → no prediction
    }
    
    // Count contacts within window in historical data
    // Normalize by number of possible windows in 7-day history
    let total_windows = Duration::days(7).as_minutes() / window_minutes as i64;
    let contact_windows = records.iter()
        .map(|r| r.start_time / (window_minutes as i64 * 60))
        .collect::<HashSet<_>>()
        .len() as u32;
    
    // EWMA with recency weighting
    let base_prob = (contact_windows * 65535) / total_windows as u32;
    
    // Recency boost: if contacted recently, increase probability
    let recency_weight = if records.last().unwrap().recency_hours() < 24 {
        1.2  // 20% boost for recent contact
    } else {
        1.0
    };
    
    ((base_prob as f64 * recency_weight) as u32).min(65535) as Probability
}
```

All arithmetic in fixed-point. The f64 multiplication in recency_weight is done offline
during a background computation cycle, not on the routing critical path.

### Delivery Probability (PRoPHET-inspired)

```
P(self → destination via next_hop h):
  = P(h, destination, T) × reachability_factor

where:
  P(h, destination, T) = contact probability of h with destination
  T = typical message delivery window for this priority class
  reachability_factor = 0.8 (conservative, accounts for h potentially being busy)
```

Implementation notes:
- We do NOT use transitive probability propagation (attack surface: route poisoning)
- We use only P values computed from our own observations of h's contacts
- P(h, destination) is observed when we see h meeting destination while we are a relay

## Link Quality Prediction

### Signal Strength Trend

```rust
pub struct LinkQualityPredictor {
    rssi_history: VecDeque<RssiSample>,  // Last 20 RSSI readings from this peer
    max_samples: usize,  // 20
}

impl LinkQualityPredictor {
    pub fn predict_survival_seconds(&self) -> u32 {
        if self.rssi_history.len() < 3 {
            return 30;  // Default: assume 30-second link stability
        }
        
        // Linear regression on RSSI trend
        let trend = self.rssi_trend_dbm_per_second();
        let current_rssi = self.rssi_history.back().unwrap().rssi_dbm;
        let threshold_rssi = -90i8;  // Typical BLE disconnect threshold
        
        if trend >= 0 {
            return 300;  // Signal improving or stable: assume 5 minutes
        }
        
        // Time until RSSI hits threshold: (threshold - current) / trend
        let seconds_to_disconnect = (threshold_rssi - current_rssi) as i32 / trend as i32;
        seconds_to_disconnect.max(0) as u32
    }
}
```

Link survival prediction is used to decide whether to send a large message on a link
that may be about to disconnect:
- Predicted survival > message_size / link_bandwidth: go ahead
- Otherwise: defer to next contact or find alternative path

## Congestion Prediction

```rust
pub struct CongestionPredictor {
    queue_depth_history: VecDeque<(i64, u32)>,  // (timestamp, queue_depth)
    max_samples: usize,  // 60 (one per minute for 1 hour)
}

impl CongestionPredictor {
    pub fn predict_queue_depth_minutes_from_now(&self, minutes: u32) -> u32 {
        if self.queue_depth_history.len() < 10 {
            return self.current_queue_depth();
        }
        
        // Simple linear extrapolation of queue trend
        let trend = self.queue_depth_per_minute();
        let current = self.current_queue_depth();
        
        (current as i64 + trend * minutes as i64).max(0) as u32
    }
    
    pub fn is_congestion_likely_soon(&self) -> bool {
        let predicted_60min = self.predict_queue_depth_minutes_from_now(60);
        predicted_60min > self.queue_capacity() * 8 / 10  // 80% threshold
    }
}
```

Congestion prediction is used by the routing layer to avoid sending to congested next-hops.
A node with predicted high congestion in the next hour should not receive additional messages
(except P0-P1 which get routed anyway).

## Battery Prediction

```rust
pub struct BatteryPredictor {
    battery_samples: VecDeque<BatterySample>,
}

pub struct BatterySample {
    pub timestamp: i64,
    pub battery_percent: u8,
    pub charging: bool,
}

impl BatteryPredictor {
    pub fn estimated_minutes_to_critical(&self, critical_threshold: u8) -> Option<u32> {
        let drain_rate = self.drain_rate_percent_per_hour()?;
        if drain_rate <= 0.0 {
            return None;  // Charging or stable
        }
        
        let current = self.current_percent();
        let headroom = current.saturating_sub(critical_threshold) as f32;
        
        Some((headroom / drain_rate * 60.0) as u32)
    }
}
```

Battery prediction informs routing decisions:
- Nodes with <30 minutes of battery predicted: avoid as next-hop for P4-P7 (may go offline)
- Exception: P0-P1 routes to any available node regardless of battery

## History Storage

```sql
-- Contact history table
CREATE TABLE contact_events (
    id INTEGER PRIMARY KEY,
    self_id BLOB NOT NULL,         -- This node's NodeId (32 bytes)
    peer_id BLOB NOT NULL,         -- Peer NodeId (32 bytes)
    contact_start INTEGER NOT NULL, -- Unix timestamp
    contact_end INTEGER NOT NULL,   -- Unix timestamp
    transport TEXT NOT NULL,        -- 'BLE' | 'WIFI' | 'LORA'
    avg_rssi INTEGER,              -- Average RSSI during contact (nullable)
    messages_exchanged INTEGER DEFAULT 0
);

-- Index for fast lookup by peer
CREATE INDEX idx_contact_peer ON contact_events(self_id, peer_id, contact_start);

-- Rolling window: delete records older than 7 days
-- Run daily via background job
DELETE FROM contact_events WHERE contact_start < (strftime('%s', 'now') - 604800);

-- Max records per pair: 100 (circular buffer behavior)
-- If >100 records: delete oldest
```

**Storage estimate:** 100 contacts × 100 records × ~100 bytes = ~1MB. Manageable.

## Calibration

L2 predictions are compared against actual delivery outcomes to calibrate accuracy:

```rust
pub struct PredictionCalibrator {
    predictions: HashMap<MessageId, Probability>,
    outcomes: HashMap<MessageId, bool>,
}

impl PredictionCalibrator {
    pub fn calibration_error(&self) -> f32 {
        // Brier score: mean squared error between predicted probability and actual outcome
        let total: f32 = self.predictions.iter()
            .filter_map(|(id, pred)| {
                self.outcomes.get(id).map(|&outcome| {
                    let pred_f = *pred as f32 / 65535.0;
                    let actual = if outcome { 1.0f32 } else { 0.0f32 };
                    (pred_f - actual).powi(2)
                })
            })
            .sum();
        total / self.predictions.len() as f32
    }
}
```

Calibration runs weekly in background. If Brier score >0.25 (poor calibration):
log a warning and consider disabling L2 for routing decisions until history improves.
