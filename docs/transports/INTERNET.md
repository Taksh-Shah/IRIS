# Internet Transport

## Overview

When a device has Internet connectivity — via cellular data, Wi-Fi to a router, or
Ethernet — IRIS treats this as the highest-bandwidth, lowest-latency transport path.
Internet transport is not just a relay backup; it is the primary long-distance carrier
when available. All IRIS protocol semantics remain unchanged; only the underlying
carrier differs.

## Protocol Stack

### QUIC (Preferred)

QUIC (RFC 9000) is IRIS's preferred Internet transport for mobile nodes:

```
Application: IRIS Protocol v1
     ↓
QUIC (stream multiplexing, 0-RTT, connection migration)
     ↓
UDP
     ↓
IP (v4 or v6)
```

Why QUIC over TCP for mobile:
- **Connection migration**: a QUIC connection survives IP address changes (Wi-Fi → cellular handoff)
  using connection IDs instead of 4-tuples. IRIS sessions survive network transitions.
- **0-RTT resumption**: reconnect to known relay with zero round-trip latency (uses session tickets).
- **Head-of-line blocking**: QUIC streams are independent; loss on one stream does not block others.
- **Built-in TLS 1.3**: no separate TLS handshake layer.

```rust
// Rust QUIC implementation via quinn crate
use quinn::{ClientConfig, Endpoint};

pub struct QuicInternetTransport {
    endpoint: Endpoint,
    relay_connections: HashMap<RelayId, quinn::Connection>,
    reconnect_policy: ReconnectPolicy,
}

impl QuicInternetTransport {
    pub async fn connect_to_relay(&mut self, relay: &RelayNode) -> Result<()> {
        let connection = self.endpoint
            .connect(relay.socket_addr, &relay.hostname)?
            .await?;
        self.relay_connections.insert(relay.id.clone(), connection);
        Ok(())
    }

    pub async fn send_message(&self, relay_id: &RelayId, msg: &IrisMessage) -> Result<()> {
        let conn = self.relay_connections.get(relay_id)
            .ok_or(TransportError::NotConnected)?;
        let mut stream = conn.open_uni().await?;
        let encoded = msg.encode_framed()?;
        stream.write_all(&encoded).await?;
        stream.finish().await?;
        Ok(())
    }
}
```

### TCP + TLS 1.3 (Fallback)

When QUIC is blocked by firewalls (UDP blocked on some enterprise networks):

```rust
pub struct TcpTlsTransport {
    connector: TlsConnector,
    connections: HashMap<RelayId, TcpStream>,
}
```

TCP limitations vs QUIC:
- No connection migration (new TCP handshake required after IP change)
- Head-of-line blocking on packet loss
- Additional TLS handshake round-trip (mitigated by TLS 1.3 0-RTT)

### WebSocket (Firewall Bypass)

When both QUIC and raw TCP fail (aggressive firewalls, hotel captive portals):

```
IRIS frames
    ↓
WebSocket frames (RFC 6455)
    ↓
HTTP/1.1 UPGRADE → WS
    ↓
TCP
    ↓
TLS (optional, wss://)
```

WebSocket traverses most firewalls because it starts as HTTP. The relay exposes
`wss://relay.iris.example/ws` as a WebSocket endpoint.

Protocol negotiation order: QUIC → TCP+TLS → WebSocket. Try each for 3 seconds before
falling back. Store which worked for a given relay in local preferences.

## IRIS Relay Server

### Role

The relay acts as a TURN-like intermediary when two IRIS nodes cannot establish a direct
IP connection (both behind NAT, different cellular networks, etc.):

```
Node A (behind Jio CGNAT)
        ↓ QUIC to relay
    IRIS Relay Server
        ↓ QUIC to node B
Node B (behind Airtel CGNAT)
```

The relay does **not** decrypt message content — E2EE ensures it sees only ciphertext.
The relay does see: sender NodeId, recipient NodeId, message size, timestamp.

### Federated Relay Model

Anyone can run an IRIS relay. The relay software is open-source and lightweight (runs
on a ₹200/month VPS). Relay discovery:

1. **DNS SRV records**: `_iris._quic.example.com SRV 10 0 7777 relay.example.com`
2. **Well-known relays**: IRIS ships with a default list of community relays
3. **Manual configuration**: user enters relay address directly
4. **Relay-of-relays**: relay nodes can advertise each other (relay gossip)

```rust
pub struct RelayNode {
    pub id: RelayId,           // SHA-256 of relay's public key
    pub hostname: String,
    pub port: u16,
    pub quic_port: u16,
    pub ws_port: u16,
    pub region: String,        // "in-west", "in-south", etc.
    pub capacity: u32,         // max concurrent connections
    pub load: f32,             // 0.0–1.0 current load
    pub latency_ms: u32,       // last measured round-trip
    pub public_key: PublicKey, // for certificate pinning
}
```

### Relay Privacy Guarantees

| What the relay sees | What the relay does NOT see |
|--------------------|----------------------------|
| Sender NodeId | Message content (E2EE) |
| Recipient NodeId | User's real name or phone number |
| Message size | Contact list |
| Timestamp | Decrypted payload |
| Source IP | Who the sender actually is |

Metadata minimization: IRIS may implement onion-style relay routing in a future version
to hide even sender/recipient from individual relays.

## NAT Traversal

### CGNAT in India

Most Indian mobile networks use Carrier-Grade NAT (CGNAT). Two phones on the same
carrier cannot reach each other via public IP — both appear to have the same
public IP. Direct peer-to-peer over Internet is not possible without a relay.

IRIS does not attempt STUN/TURN hole-punching (too unreliable in India's CGNAT
environment). Instead:
1. Always route via relay for Internet traffic
2. Reserve direct IP for cases where both nodes have public IPs (desktop servers,
   edge nodes with static IPs)

### Relay-Free Direct IP

For nodes with public IPs (static IP VPS, edge nodes, command centers):

```rust
pub async fn attempt_direct_connection(
    &self,
    target: &NodeAdvertisement,
) -> Option<QuicConnection> {
    if let Some(public_addr) = target.public_ip_addr {
        match timeout(Duration::from_secs(3),
            self.endpoint.connect(public_addr, &target.hostname)
        ).await {
            Ok(Ok(conn)) => return Some(conn),
            _ => {}
        }
    }
    None  // Fall back to relay
}
```

## IPv4/IPv6 Dual-Stack

IRIS operates on both IPv4 and IPv6. Preference: IPv6 when available (avoids CGNAT,
no NAT traversal needed in theory). Happy Eyeballs algorithm (RFC 8305): race IPv4
and IPv6 connections, use whichever succeeds first (with 50ms preference for IPv6).

India IPv6 status: Jio is fully dual-stack (IPv6-first since 2018). Airtel: dual-stack
in major cities. BSNL: IPv4-only on many circuits.

## Connection Pooling

```rust
pub struct ConnectionPool {
    connections: HashMap<RelayId, Vec<PooledConnection>>,
    max_per_relay: usize,   // default: 3
    idle_timeout: Duration, // default: 90 seconds
}

// Reuse existing connection; open new one if all busy or idle timeout exceeded
impl ConnectionPool {
    pub async fn get_or_create(&mut self, relay: &RelayId) -> Result<PooledConnection> {
        let pool = self.connections.entry(relay.clone()).or_default();
        if let Some(conn) = pool.iter().find(|c| !c.is_busy() && !c.is_expired()) {
            return Ok(conn.clone());
        }
        let new_conn = self.open_new_connection(relay).await?;
        pool.push(new_conn.clone());
        Ok(new_conn)
    }
}
```

## Reconnection Logic

Exponential backoff with jitter on connection failure:

```
attempt 1: wait 1s
attempt 2: wait 2s
attempt 3: wait 4s
attempt 4: wait 8s
attempt 5: wait 16s
attempt 6+: wait 30s (cap)
jitter: ±25% of wait time to prevent thundering herd
```

On network change events (detected via ConnectivityManager on Android, NWPathMonitor
on iOS): immediately attempt reconnection without waiting for backoff.

## Mesh Fallback When Relay Unavailable

If all known relays are unreachable:
1. Log relay failure with timestamp
2. Switch to mesh-only mode: BLE + Wi-Fi Direct + LoRa
3. Retry relay connections every 60 seconds in background
4. Surface relay status in UI: "Relay unavailable — mesh only"

## Cost Model

IRIS philosophy: minimize relay bandwidth cost. Most traffic stays on local mesh.
Only relay traffic that cannot be delivered via mesh.

Relay bandwidth estimate per active node per day:
- P0–P2 messages: ~10KB (rare, high priority)
- P3 messages: ~50KB (location check-ins, status)
- P4–P7 messages: ~0KB via relay (mesh only, not relayed)

A relay server handling 1,000 active nodes during emergency: ~60MB/day of relay traffic.
Cost: ~₹5/day on typical Indian VPS pricing.

## QUIC Connection Migration

When mobile device changes IP (Wi-Fi → cellular):

```
Before: QUIC connection from 192.168.1.100:12345 → relay 203.0.113.5:7777
Phone switches to 4G: new IP 100.64.0.5:54321
After:  QUIC connection migrates to 100.64.0.5:54321 → relay 203.0.113.5:7777
        (same QUIC connection ID, no re-handshake, in-flight messages delivered)
```

This is automatic in QUIC. TCP would lose the connection and require full reconnect
(including TLS handshake, losing any in-flight messages).
