# IRIS Telemetry Design

**Document Type:** Operations Reference  
**Status:** DRAFT  
**Audience:** Engineers, privacy reviewers, operators

---

## 1. Telemetry Philosophy

IRIS telemetry is **opt-in, minimal, and anonymous**. The product must work without any telemetry. Telemetry exists solely to help the engineering team improve the protocol and diagnose systemic issues that cannot be observed otherwise.

Core principles:
1. **No telemetry without explicit consent** — user must opt in, not opt out
2. **Minimum viable collection** — collect only what is necessary for stated purpose
3. **No individual identification** — telemetry cannot identify or track individual users
4. **No message intelligence** — zero message content, sender, recipient, or conversation metadata
5. **Transparent** — user can see exactly what will be sent before consenting
6. **Revocable** — user can withdraw consent at any time; prior data purged on request
7. **Local-first** — telemetry batches locally; upload only when explicitly triggered or on schedule with connectivity

---

## 2. Opt-In Flow

### 2.1 Initial Consent

On first launch, after basic setup is complete:

```
Screen: "Help improve IRIS (Optional)"

IRIS can share anonymous usage statistics to help us improve
message delivery and battery life. No messages, no contacts,
no location are ever shared.

[What will be shared]  <- expands to full list
[What will NEVER be shared]  <- expands to exclusion list

[Skip]          [Enable Anonymous Statistics]
```

Consent is recorded as a boolean in local preferences with a timestamp. No consent = no telemetry. Zero implicit telemetry.

### 2.2 Transparency Screen

The "What will be shared" expansion shows the exact JSON schema of what is uploaded, with example values. Non-technical users see a plain-English summary; a "Show technical details" toggle shows the raw schema.

### 2.3 Withdrawal

Settings > Privacy > Anonymous Statistics > Disable  
On disable: local telemetry buffer is deleted. A deletion request is queued for the server (purge all records associated with this device's telemetry ID). Telemetry ID is rotated on re-enable.

---

## 3. What IS Collected

### 3.1 Session Summary (uploaded once per day when online)

```json
{
  "schema_version": "1.0",
  "telemetry_id": "<rotating anonymous ID>",
  "session_date": "2025-01-01",
  "platform": "android",
  "app_version": "0.5.0",
  "os_version": "14",
  "device_class": "budget",

  "network": {
    "sessions_with_mesh": 3,
    "total_mesh_minutes": 145,
    "avg_neighbors": 2.3,
    "max_neighbors": 7,
    "partitioned_minutes": 12
  },

  "messages": {
    "sent": 14,
    "received": 22,
    "relayed": 87,
    "delivery_ratio": 0.86,
    "latency_p50_ms": 340,
    "latency_p95_ms": 2100,
    "dropped_ttl": 3,
    "dropped_queue": 0,
    "store_carry_delivered": 2
  },

  "transports": {
    "ble_minutes": 145,
    "wifi_minutes": 42,
    "lora_minutes": 0,
    "internet_minutes": 23
  },

  "battery": {
    "background_drain_pct_per_hour": 1.8,
    "foreground_drain_pct_per_hour": 4.2
  },

  "routing": {
    "algorithm": "prophet_hybrid",
    "routing_decisions": 234,
    "table_size_avg": 18
  },

  "errors": {
    "ble_errors": 2,
    "crypto_errors": 0,
    "storage_errors": 0,
    "crash_count": 0
  },

  "region": "IN-GJ"
}
```

**`telemetry_id`**: A random UUID generated on first opt-in. Rotated every 30 days. Never linked to user identity, device ID, phone number, or any PII.

**`device_class`**: Bucketed by RAM — "budget" (<3GB), "mid" (3–6GB), "high" (>6GB). Never the device model.

**`region`**: ISO 3166-2 state code (e.g., IN-GJ for Gujarat). Never city, district, or coordinates.

### 3.2 Routing Events (anonymized, sampled)

A 5% sample of routing decisions (not messages, just routing algorithm events):

```json
{
  "event_type": "routing_decision",
  "telemetry_id": "<rotating anonymous ID>",
  "timestamp_bucket": "2025-01-01T14:00:00Z",
  "message_priority": "normal",
  "routing_algorithm": "prophet",
  "decision": "forward",
  "hop_count": 2,
  "neighbors_considered": 3,
  "delivery_probability_used": 0.74
}
```

Note: No message ID, no node IDs, no content-derived fields.

### 3.3 Error Reports (on crash or ERROR-level event)

```json
{
  "event_type": "error",
  "telemetry_id": "<rotating anonymous ID>",
  "timestamp": "2025-01-01T14:23:00Z",
  "app_version": "0.5.0",
  "error_category": "ble_connection_failure",
  "error_code": "GATT_CONN_TIMEOUT",
  "stack_trace_hash": "sha256:abc...",
  "platform": "android",
  "os_version": "14"
}
```

Stack traces are hashed, never sent raw (stack traces can contain PII in some frameworks).

---

## 4. What is NEVER Collected

This is an exhaustive list of excluded data categories. Any new telemetry field must be reviewed against this list.

| Category | Examples |
|---|---|
| Message content | Text, attachments, any message body |
| Message metadata | Sender, recipient, timestamp of specific message, channel name |
| Contact lists | Who the user communicates with |
| Identity information | Node public key, user name, phone number, email |
| Location | GPS coordinates, precise address, building |
| Social graph | Who knows whom, community membership |
| Message count per conversation | Any data enabling inference of relationship |
| Device identifiers | IMEI, Android ID, Advertising ID, IDFV, IDFA |
| IP addresses | Source or destination of any connection |
| Network identifiers | SSID, BSSID, BLE device address |
| Biometric data | Any data about the user's physical characteristics |
| Financial data | Any payment or commercial information |
| Health data | Any health or medical information |

---

## 5. Telemetry Data Lifecycle

### 5.1 On-Device

1. Metrics are written to a local telemetry buffer (SQLite, separate DB from messages)
2. Buffer is capped at 7 days of data or 10MB, whichever is smaller
3. Oldest entries dropped when buffer is full
4. No telemetry data leaves the device without an explicit upload trigger

### 5.2 Upload Triggers

- **Scheduled**: Once per 24 hours, when online, WiFi preferred
- **Manual**: User explicitly taps "Send anonymous report" in settings
- **Never**: During mesh-only operation (no internet) — data stays local

### 5.3 Server-Side Retention

- Raw telemetry: 90 days
- Aggregated statistics: indefinite (no individual records)
- Deletion requests: processed within 30 days

### 5.4 Server-Side Processing

1. Receive batch upload
2. Validate schema version
3. Validate telemetry_id format (never link to user identity)
4. Store in time-series DB
5. Aggregate into daily/weekly rollups
6. Delete raw after 90 days

---

## 6. Telemetry Transport

### 6.1 Protocol

- HTTPS POST to `telemetry.iris-project.in/v1/batch`
- JSON payload, gzip compressed
- Maximum batch size: 256KB
- Timeout: 30 seconds

### 6.2 Request Format

```json
{
  "batch_id": "<random UUID>",
  "uploaded_at": "2025-01-01T14:00:00Z",
  "records": [
    { ... session summary ... },
    { ... routing event ... }
  ]
}
```

### 6.3 Authentication

No authentication required for telemetry upload. The telemetry server accepts anonymous POSTs. This prevents any possibility of linking uploads to authenticated accounts.

Rate limiting: max 10 uploads per telemetry_id per day (server-side enforcement).

### 6.4 Failure Handling

If upload fails: retry with exponential backoff (1m, 5m, 15m, 1h, 24h). After 7 days without successful upload, records are deleted locally rather than accumulated.

---

## 7. Privacy Review

### 7.1 Data Minimization Assessment

For each field in the telemetry schema, the privacy reviewer must confirm:
- **Necessity**: Is this field necessary for the stated engineering purpose?
- **Proportionality**: Is the granularity proportional to the need? (e.g., region, not city)
- **Alternative**: Could the same insight be gained without this field?

### 7.2 Re-identification Risk Assessment

Combination attacks must be considered. Even individually anonymous fields can combine to re-identify. Risk assessment matrix:

| Field Combination | Re-identification Risk | Mitigation |
|---|---|---|
| region + device_class + app_version + date | Low (millions of possible combinations) | Accept |
| region + session_date + neighbor_count | Medium | Neighbor count is averaged, not exact |
| telemetry_id over time | High if ID not rotated | 30-day rotation enforced |
| error stack trace | High (can leak code paths with user data) | Hash only, never raw |

### 7.3 Third-Party Access

Telemetry data is never sold, shared with advertising networks, shared with data brokers, or provided to third parties except:
- Aggregated, anonymized statistics in public project reports
- As required by law (government order) — only with legal review

---

## 8. GDPR Compliance Implications

Although IRIS is India-first, GDPR may apply if:
- EU users use the app (extraterritorial scope of GDPR Art. 3)
- The telemetry ID, even rotated, could be considered personal data under GDPR's broad definition

### 8.1 Legal Basis

If telemetry ID is personal data under GDPR: **consent** is the legal basis (Art. 6(1)(a)). The opt-in flow satisfies GDPR consent requirements (freely given, specific, informed, unambiguous).

### 8.2 Data Subject Rights under GDPR

| Right | Implementation |
|---|---|
| Right to access | User can view exactly what telemetry they've sent (local buffer) |
| Right to erasure | Opt-out + deletion request; server deletes within 30 days |
| Right to portability | Local buffer can be exported as JSON |
| Right to withdraw consent | Settings > Privacy > Disable, immediate effect |

### 8.3 PDPA (India) Compliance

Under the Digital Personal Data Protection Act 2023:
- Consent obtained before collection: Yes (opt-in flow)
- Purpose stated at collection: Yes (transparency screen)
- Data minimization: Yes (design principle)
- Storage limitation: 90 days raw, then aggregated only
- Data fiduciary obligations: Apply to IRIS project entity

**Open question for legal review**: Does the rotating telemetry_id constitute personal data under PDPA 2023? The answer affects whether PDPA consent requirements apply to telemetry specifically. See `docs/legal/PRIVACY_REQUIREMENTS.md`.

---

## 9. Engineering Telemetry (Internal Debug Builds Only)

Debug builds (never distributed to users) may enable additional telemetry for engineering purposes:
- Detailed routing decision logs
- Full stack traces (not hashed)
- Per-message delivery timing (with synthetic test messages only)
- Transport negotiation logs

This data stays on the test device and is never uploaded to telemetry servers. Exported only via explicit developer action (adb pull or Instruments capture).
