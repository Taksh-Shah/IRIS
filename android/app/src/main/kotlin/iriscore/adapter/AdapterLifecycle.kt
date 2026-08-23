package iriscore.adapter

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import java.util.concurrent.TimeoutException
import kotlinx.coroutines.TimeoutCancellationException

/**
 * AC-5 FFI-contract absorption layer.
 *
 * Implements the carry-forward requirements from the transport nodes that the
 * Android adapter surfaces must honor (carried from WIFIAWARE-001 / WIFIDIRECT-001
 * security reviews and recorded in ANDROID_DESIGN.md §5):
 *
 * | Requirement  | Primitive here |
 * |--------------|----------------|
 * | NEW-WA-RT-108 | [VerifiedPeerCache] — `connect()` NDP-reuse key switches to the VERIFIED PeerId once envelope resolution exists (candidate-only today, DEC-WA-0007). |
 * | NEW-WA-RT-109 | [RadioStateTracker] (multi-subscriber availability stream) + [NdpRegistry] (closed-NDP prune). |
 * | NEW-WA-RT-110 | [FfiCallTimeout] — per-call FFI timeouts (platform calls seconds-scale worst case). |
 * | NEW-WA-RT-111 / WIFIDIRECT RT-010 | [SessionGate] — idempotent start/subscribe, same-session reuse, `ensure_started()` NOT latched into failure. |
 * | NEW-WA-RT-112 | [RingBufferOutbox] — bounded ring buffer, oldest-drop, per-destination eviction (wifi_direct outbox pattern, RT-002). |
 *
 * All primitives are plain JVM + kotlinx.coroutines (no Android framework
 * dependency) so they are unit-testable on the host JVM (Gradle `test` leg;
 * the Gradle/Android compile leg is environment-gated on the integration host).
 */

// ---------------------------------------------------------------------------
// NEW-WA-RT-110 — per-call FFI timeouts
// ---------------------------------------------------------------------------

/**
 * Per-call FFI timeout enforcement (NEW-WA-RT-110).
 *
 * Android platform calls can block for seconds-scale worst case. Both adapter
 * paths are covered:
 *  - `suspendCall` wraps suspend adapter ops in `withTimeout` (the Rust side
 *    awaits the generated Kotlin `suspend fun`; the coroutine is parked).
 *  - `syncCall` runs a blocking adapter op on a watchdog dispatcher and returns
 *    `onTimeout` if it overruns.
 *
 * NOTE: a blocking platform call that has already started cannot be forcibly
 * interrupted; the caller is released after the timeout and the platform call
 * is observed asynchronously. The timeout is a liveness guarantee, not a
 * cancellation guarantee.
 */
object FfiCallTimeout {

    /** Default per-call budget (platform calls are seconds-scale worst case). */
    const val DEFAULT_TIMEOUT_MS = 30_000L

    /** Short budget for idempotence/state queries (never blocks on the radio). */
    const val SHORT_TIMEOUT_MS = 5_000L

    private val watchdogScope = CoroutineScope(SupervisorJob() + Dispatchers.Default)

    /**
     * Run a `suspend` platform op under a timeout. Re-throws the platform
     * exception on early completion, or `TimeoutException` on overrun.
     */
    suspend fun <T> suspendCall(
        timeoutMs: Long = DEFAULT_TIMEOUT_MS,
        block: suspend () -> T,
    ): T = withTimeout(timeoutMs) { block() }

    /**
     * Run a synchronous platform op under a timeout. On overrun `onTimeout` is
     * returned (the caller decides whether that is an error fallback or a
     * benign default). Must not be invoked from inside another `runBlocking`.
     */
    fun <T> syncCall(
        timeoutMs: Long = DEFAULT_TIMEOUT_MS,
        onTimeout: T,
        block: () -> T,
    ): T {
        val deferred = watchdogScope.async { block() }
        return try {
            runBlocking { withTimeout(timeoutMs) { deferred.await() } }
        } catch (_: TimeoutCancellationException) {
            onTimeout
        }
    }

}

// ---------------------------------------------------------------------------
// NEW-WA-RT-111 / WIFIDIRECT RT-010 — idempotent start/subscribe
// ---------------------------------------------------------------------------

/**
 * Idempotent start/subscribe gate (NEW-WA-RT-111 / WIFIDIRECT RT-010).
 *
 * `ensureStarted()`:
 *  - returns the live resource on re-entry (same-session reuse — the platform
 *    must NOT create a new DiscoverySession per `subscribe`, and must not
 *    re-broadcast the same DNS-SD / NAN session);
 *  - coalesces concurrent starters through an internal mutex;
 *  - NEVER latches into failure: a failed `create` clears the latch so the next
 *    call attempts a fresh platform bring-up (mirrors `ensure_started()` not
 *    being latched on the transport side);
 *  - invalidation ([invalidate]) takes the SAME mutex as [ensureStarted]
 *    (AND-RT-105) so a platform channel-lost callback can never interleave with
 *    an in-flight create; the create result is double-checked so a stale create
 *    cannot overwrite an invalidation.
 *
 * The gate is generic over the created session/handle type.
 */
class SessionGate<T>(private val create: suspend () -> T) {

    private val mutex = Mutex()
    private var live: T? = null

    /** Returns the live resource; starts it on first (or first-after-failure) use. */
    suspend fun ensureStarted(): T = mutex.withLock {
        val existing = live
        if (existing != null) return@withLock existing
        val created = try {
            create()
        } catch (e: Throwable) {
            // NEW-WA-RT-111: not latched into failure.
            live = null
            throw e
        }
        // AND-RT-105: re-check after create — a concurrent invalidation cannot
        // be overwritten by a stale create result (single-lock discipline).
        if (live == null) {
            live = created
        }
        created
    }

    /** Drops the live resource; `true` if one was running (used by shutdown paths). */
    suspend fun reset(): Boolean = mutex.withLock {
        val hadLive = live != null
        live = null
        hadLive
    }

    /**
     * Seeds the live resource from an asynchronous platform callback (e.g.
     * `onSessionStarted`). Never downgrades an already-live resource.
     */
    suspend fun markStarted(resource: T): Boolean = mutex.withLock {
        if (live == null) {
            live = resource
            true
        } else {
            false
        }
    }

    /** True when a live resource is held (never calls the platform). */
    suspend fun isStarted(): Boolean = mutex.withLock { live != null }

    /**
     * Invalidates the live resource under the SAME mutex as [ensureStarted]
     * (AND-RT-105) — a racing re-create is serialized, so a stale create cannot
     * resurrect an invalidated resource. Suspend; platform callbacks (e.g.
     * channel lost) dispatch through a coroutine scope.
     */
    suspend fun invalidate(): Boolean = mutex.withLock {
        val hadLive = live != null
        live = null
        hadLive
    }
}

// ---------------------------------------------------------------------------
// NEW-WA-RT-112 — ring-buffer outbox
// ---------------------------------------------------------------------------

/**
 * Bounded ring-buffer outbox (NEW-WA-RT-112).
 *
 * Replaces the remove-0/rebuild queue: bounded, oldest-drop on overrun, and
 * **per-destination eviction** so one heavily-saturated peer cannot starve
 * another destination (exact pattern fixed in WIFIDIRECT-001 RT-002
 * `outbox_eviction_keeps_other_destinations`).
 *
 *  - Each destination queue is capped at [capacityPerDestination]; overflow
 *    drops that destination's oldest frame.
 *  - Destination slots are strictly bounded at [maxDestinations] (AND-RT-109):
 *    when exceeded the least-recently-*used* destination is evicted even when
 *    its queue is non-empty — its oldest head is dropped first, and the whole
 *    queue when the slot is still over budget.
 */
class RingBufferOutbox<T>(
    private val capacityPerDestination: Int = 64,
    private val maxDestinations: Int = 128,
) {

    // Access-order map: iteration is least-recently-accessed -> most-recently-accessed.
    private val queues = object : LinkedHashMap<Long, ArrayDeque<T>>(0, 0.75f, true) {}

    @Synchronized
    fun enqueue(destination: Long, item: T): Boolean {
        val queue = queues.getOrPut(destination) { ArrayDeque(capacityPerDestination + 1) }
        if (queue.size >= capacityPerDestination) {
            // Oldest-drop for THIS destination only (RT-002).
            queue.removeFirst()
        }
        queue.addLast(item)
        // Strict destination bound (AND-RT-109): evict the LRU destination even
        // when non-empty — drop its oldest head, then the whole queue when
        // repeatedly over budget.
        while (queues.size > maxDestinations) {
            val lru = queues.entries.firstOrNull() ?: break
            if (lru.value.isNotEmpty()) lru.value.removeFirst()
            if (lru.value.isEmpty()) queues.remove(lru.key)
        }
        return true
    }

    /** Removes and returns all frames for [destination] (never rebuilds a cleared queue). */
    @Synchronized
    fun drain(destination: Long): List<T> =
        queues.remove(destination)?.toList() ?: emptyList()

    /** Discards the destination's queue outright (closed-destination cleanup, AND-RT-109). */
    @Synchronized
    fun drainDestination(destination: Long) {
        queues.remove(destination)
    }

    @Synchronized
    fun sizeFor(destination: Long): Int = queues[destination]?.size ?: 0

    @Synchronized
    fun totalSize(): Int = queues.values.sumOf { it.size }

    @Synchronized
    fun destinations(): Set<Long> = queues.keys.toSet()
}

// ---------------------------------------------------------------------------
// NEW-WA-RT-109 — availability stream + closed-NDP prune
// ---------------------------------------------------------------------------

/**
 * Multi-subscriber radio availability stream (NEW-WA-RT-109).
 *
 * Exposes availability as a replaying [StateFlow] — a rebound transport and
 * every UI subscriber observe all *future* transitions (the single-`take()`
 * one-shot hole from NEW-WA-RT-109 is closed by construction). Adapters also
 * own the pull-only `isAvailable()` projection surfaced to Rust (proj-WA-1).
 */
class RadioStateTracker {

    private val _available = MutableStateFlow(false)

    /** Replaying, multi-subscriber availability signal. */
    val available: StateFlow<Boolean> = _available.asStateFlow()

    fun setAvailable(available: Boolean) {
        if (_available.value != available) _available.value = available
    }

    /** Pull-only projection for the Rust `is_available()` bridge (proj-WA-1). */
    fun isAvailable(): Boolean = _available.value
}

/**
 * Open-NDP registry with **closed-NDP prune** (NEW-WA-RT-109).
 *
 * Frames whose NDP/data-path handle was closed are dropped at the drain
 * boundary rather than forwarded with a dead handle.
 */
class NdpRegistry {

    private val open = HashSet<Long>()

    @Synchronized
    fun opened(handle: Long) {
        open.add(handle)
    }

    @Synchronized
    fun closed(handle: Long) {
        open.remove(handle)
    }

    @Synchronized
    fun closeAll(): List<Long> {
        val all = open.toList()
        open.clear()
        return all
    }

    /** RT-109: reject payloads for a closed/pruned NDP. */
    @Synchronized
    fun accepts(handle: Long): Boolean = open.contains(handle)

    @Synchronized
    fun openHandles(): Set<Long> = open.toSet()

    @Synchronized
    fun openCount(): Int = open.size
}

// ---------------------------------------------------------------------------
// NEW-WA-RT-108 — verified-peer reuse key
// ---------------------------------------------------------------------------

/**
 * Candidate -> VERIFIED PeerId reuse key (NEW-WA-RT-108, DEC-WA-0007).
 *
 * Today the candidate peer identity is a platform short-id/handle (candidate-
 * only trust). Once a real envelope is resolved into a VERIFIED 64-hex PeerId,
 * the adapter remembers the binding so the application's connect/reuse key can
 * switch to the verified identity — eliminating candidate-level collisions for
 * the actual mesh link.
 */
class VerifiedPeerCache {

    private val verifiedByHandle = HashMap<Long, String>()
    private val handleByVerified = HashMap<String, Long>()

    @Synchronized
    fun rememberVerified(platformHandle: Long, verifiedPeerIdHex: String) {
        handleByVerified.remove(verifiedPeerIdHex)?.let { oldHandle ->
            if (oldHandle != platformHandle) verifiedByHandle.remove(oldHandle)
        }
        verifiedByHandle[platformHandle] = verifiedPeerIdHex
        handleByVerified[verifiedPeerIdHex] = platformHandle
    }

    @Synchronized
    fun verifiedPeerIdFor(platformHandle: Long): String? = verifiedByHandle[platformHandle]

    @Synchronized
    fun reuseHandleFor(verifiedPeerIdHex: String): Long? = handleByVerified[verifiedPeerIdHex]

    @Synchronized
    fun evict(platformHandle: Long) {
        verifiedByHandle.remove(platformHandle)?.let {
            if (handleByVerified[it] == platformHandle) handleByVerified.remove(it)
        }
    }

    @Synchronized
    fun size(): Int = verifiedByHandle.size
}

// ---------------------------------------------------------------------------
// AND-RT-108 — discovery-beacon candidate PeerId
// ---------------------------------------------------------------------------

/**
 * Best-effort candidate 64-hex PeerId from an IRIS discovery beacon
 * (`service_specific_info` / DNS-SD TXT record). Wire layout (mirrors
 * `wifiaware_beacon.rs`): `[0] version, [1..3] caps, [3] kind, [4..20]
 * peer_short = SHA-256(pubkey)[..16], [20..22] freshness` (22-byte prefix,
 * Android may pad past it). The beacon carries only the *short* id — a
 * candidate hint, never a trust boundary (DEC-WA-0007); the full identity is
 * resolved on the verified advertisement/envelope path. Returns `null` for
 * malformed/truncated payloads.
 */
internal fun beaconCandidatePeerIdHex(beacon: ByteArray): String? {
    if (beacon.size < 22) return null
    if (beacon[0].toInt() != 1) return null
    if (beacon[3].toInt() != 0) return null
    val padded = ByteArray(32)
    beacon.copyInto(padded, 0, 4, 20)
    return iriscore.util.PeerIdCodec.toHex(padded)
}