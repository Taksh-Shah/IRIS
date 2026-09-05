package iriscore.adapter

import android.bluetooth.BluetoothAdapter
import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.bluetooth.BluetoothProfile
import android.bluetooth.BluetoothStatusCodes
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import android.os.Build
import android.os.ParcelUuid
import android.os.SystemClock
import android.bluetooth.le.AdvertiseCallback
import android.bluetooth.le.AdvertiseData
import android.bluetooth.le.AdvertiseSettings
import android.bluetooth.le.BluetoothLeAdvertiser
import android.bluetooth.le.BluetoothLeScanner
import android.bluetooth.le.ScanCallback
import android.bluetooth.le.ScanFilter
import android.bluetooth.le.ScanResult as AndroidScanResult
import android.bluetooth.le.ScanSettings
import iriscode.FfiAcceptedConnection
import iriscode.FfiAdvertisementData
import iriscode.FfiBleAdapter
import iriscode.FfiGattWriteEvent
import iriscode.FfiScanFilter
import iriscode.FfiScanResult
import iriscode.AdapterOff
import iriscode.DeviceNotFound
import iriscode.Timeout
import iriscode.GattFailure
import iriscode.InvalidArgument
import iriscode.PermissionDenied
import iriscode.Transport
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.ConcurrentSkipListSet
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/**
 * 10-op `FfiBleAdapter` foreign-trait implementation (Android platform).
 *
 * Lifecycle behavior per ANDROID_DESIGN.md §5 / AC-5:
 *  - every call runs under [FfiCallTimeout] (NEW-WA-RT-110);
 *  - inbound data is DRAIN-based (no `MutexGuard` crosses the FFI boundary —
 *    proj-BLE-1); the Rust bridge consumes `incomingGattWrites()`/`scanResults()`;
 *  - scan restart is guard-bounded at 5 per 30-s window (Android 8+ limit);
 *    the core's own `scan_allowed()` backoff remains authoritative (BLE-001).
 *
 * Session handles are adapter-owned monotonically-increasing ids; every
 * platform object is keyed by handle in `sessions` so nothing is smuggled
 * across the FFI boundary.
 *
 * NOTE: the Gradle/Android compile + device legs are ENV-GATED on the
 * integration host (no Android SDK/NDK/kotlinc); this source is the AC-5
 * deliverable and is verified by the CI/Gradle leg (AC-6/AC-11).
 */
class AndroidBleTransportAdapter(context: Context) : FfiBleAdapter {

    companion object {
        /**
         * IRIS BLE transport service UUID (BLE-001).
         *
         * CROSS-001: must byte-match `iris_core::transport::ble::IRIS_SERVICE_UUID`
         * (`01000000-0000-0000-0000-000000000000`) exactly — this is what iOS
         * advertises too (`IrisBleConstants.serviceUUID`). Android used to define
         * its own independent value (`3e5c6b1a-...-0c0d`) here, so an Android
         * scanner filtering on that UUID never matched an iOS peripheral
         * advertising Rust core's UUID, and vice versa — Android and iOS were
         * mutually invisible to each other's BLE discovery. Do not change this
         * without also updating the other two definitions.
         */
        val IRIS_SERVICE_UUID: UUID = UUID.fromString("01000000-0000-0000-0000-000000000000")

        /** GATT write characteristic used by the core BLE-001 transport. */
        val IRIS_CHARACTERISTIC_UUID: UUID = UUID.fromString("3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0e")

        /**
         * Must byte-match `iris_core::transport::ble::IRIS_IDENTIFY_CHARACTERISTIC`
         * (`Uuid([2, 0, 0, ...])` -> `02000000-0000-0000-0000-000000000000`).
         * HV-21/DEC-BLE-0008: read-only, served with this node's own
         * beacon (rebuilt with the real Wi-Fi Direct MAC folded in once
         * known — `set_identify_payload`, called from `ble.rs::
         * start_advertising`). Previously iOS-only (DEC-BLE-002-0002,
         * CoreBluetooth can't put service data in an advertisement at all);
         * Android now serves it too so a peer can learn our Wi-Fi Direct
         * MAC without the 31-byte legacy advertising ceiling.
         */
        val IRIS_IDENTIFY_CHARACTERISTIC_UUID: UUID = UUID.fromString("02000000-0000-0000-0000-000000000000")

        /**
         * HW-6: bounded retry for the legacy (API <33) `writeCharacteristic`
         * boolean-return path — a `false` return means another GATT
         * operation was still in flight, not a dead link. A handful of
         * short retries covers the normal case (the prior operation's
         * callback fires within a few ms); if it's still refusing after
         * this many attempts, something IS actually wrong and the caller's
         * own retry/backoff takes over instead.
         */
        private const val GATT_WRITE_RETRY_ATTEMPTS = 5
        private const val GATT_WRITE_RETRY_DELAY_MS = 40L

        /**
         * HV-92: Android's first `onServicesDiscovered` after a cold connect can
         * return an incomplete GATT database on several OEM stacks — the IRIS
         * service simply isn't in `gatt.services` yet. Re-run `discoverServices()`
         * a few times before declaring the connection unusable, so `connectGatt`
         * only reports ready when a write can actually land. Field guidance
         * (Nordic Android-BLE-Library, Punch Through) is 1–3 retries with a
         * short delay.
         */
        private const val SERVICE_DISCOVERY_RETRY_ATTEMPTS = 3
        private const val SERVICE_DISCOVERY_RETRY_DELAY_MS = 300L

        /**
         * HV-99: how long `connectGatt` waits for STATE_CONNECTED + service
         * discovery before giving up. Android's own `autoConnect=false` direct
         * connect has a fixed ~30 s stack timeout, which is far too long for a
         * discovery-driven reconnect probe — a peer that is rebooting Bluetooth
         * or briefly out of range blocks the discovery loop for 30 s per
         * attempt (HV-31 attempt 3, `session-14-hv31c`:
         * `connect_failed elapsed_ms=30002`). 12 s covers the HV-91 cold-start
         * first connect (~6.5 s measured) with margin and lets the 3 s
         * `active_scan_interval` retry own the cadence instead.
         */
        private const val CONNECT_READY_TIMEOUT_MS = 12_000L

        // HW-7: how long gattWrite() blocks waiting for the async
        // onCharacteristicWrite completion callback before giving up.
        // Generous relative to normal BLE write latency (single-digit to
        // low-double-digit ms) so it only trips on a genuinely stuck link.
        private const val GATT_WRITE_TIMEOUT_MS = 5000L

        /**
         * Beacon framing: the discovery beacon (DiscoveryBeacon::build,
         * ble_advert.rs — 22 bytes, BEACON_LEN) used to ride Service Data
         * keyed by [IRIS_SERVICE_UUID]. A 128-bit UUID costs 16 bytes on air;
         * 16 (UUID) + 22 (beacon) + 2 (AD header) = 40 bytes already exceeds
         * legacy BLE advertising's 31-byte total budget before the mandatory
         * Flags AD structure is even counted — startAdvertising() failed
         * with ADVERTISE_FAILED_DATA_TOO_LARGE (errorCode 1) on every real
         * device, silently, because AdvertiseCallback.onStartFailure was
         * `= Unit`. No IRIS beacon has ever actually been broadcast.
         * Manufacturer Specific Data costs a 2-byte company id instead of a
         * 16-byte UUID: 2 + 22 + 2 = 26 bytes, comfortably inside the 31-byte
         * budget on every BLE 4.0+ device (no BLE 5 extended-advertising
         * hardware required, confirmed absent on one of the two physical
         * test devices via BluetoothAdapter.isLeExtendedAdvertisingSupported).
         * 0xFFFF is the Bluetooth SIG's reserved "for testing" company id
         * (Assigned Numbers) — a real registered company id is needed before
         * this ships to production, since 0xFFFF is not collision-safe
         * against other apps/devices using the same reserved id nearby.
         */
        const val IRIS_MANUFACTURER_ID = 0xFFFF

        /** Adapter-side guard ceiling (Android 8+ throttling); core backoff is authoritative. */
        const val SCAN_RESTART_CEILING = 5
        const val SCAN_WINDOW_MS = 30_000L

        /** Ceiling on undrained inbound items per buffer (oldest-drop). */
        const val MAX_PENDING = 512

        /**
         * HV-10: bounded exponential backoff for retrying a `startAdvertising`
         * the platform refused asynchronously via `onStartFailure`. Retryable
         * codes are `TOO_MANY_ADVERTISERS` (2 — usually clears once the OS
         * reaps a leaked set), `ALREADY_STARTED` (3) and `INTERNAL_ERROR` (4).
         * `DATA_TOO_LARGE` (1) and `FEATURE_UNSUPPORTED` (5) are terminal —
         * retrying cannot help. After the last attempt the adapter reports
         * event `1` to the core (`drainAdapterEvents`).
         */
        private val ADVERTISE_RETRY_DELAYS_MS = longArrayOf(1_000L, 4_000L, 10_000L)

        /** HV-27: how often the stale-link liveness cross-check runs. */
        private const val LIVENESS_TICK_MS = 7_000L

        // HV-10/HV-31: adapter-lifecycle event codes surfaced to the core via
        // drainAdapterEvents() — mirror `iris_core::transport::ble` docs.
        private const val EVT_ADVERTISING_GAVE_UP = 1
        private const val EVT_BLUETOOTH_OFF = 2
        private const val EVT_BLUETOOTH_ON = 3

        // ---- HV-109: process-wide GATT server -----------------------------
        // The GATT *server* (and everything a remote central's connection to it
        // depends on) is a process resource, not an engine resource. Closing it
        // on `stopMesh` / re-opening it on `startMesh` was wrong on two counts:
        //   (1) the snippet builds a NEW AndroidBleTransportAdapter on every
        //       startMesh, so the OLD adapter's server leaked (`serverIf` 8 seen
        //       alive next to 12 in logcat) and a central still connected to it
        //       kept writing into a queue nobody drained;
        //   (2) closing a peripheral's GATT server does NOT drop the central's
        //       ACL link (Bluetooth core spec — only the central can), so the
        //       central keeps a STALE handle map to a database that just changed
        //       — exactly what the spec's Service Changed characteristic (0x2A05)
        //       exists to signal, and Android never sends it for us.
        // Fix: one server for the life of the process, opened once, never closed
        // here. Its inbound queues + the callback live here too so writes route
        // regardless of which adapter instance is currently "live".
        @Volatile
        private var sharedGattServer: BluetoothGattServer? = null

        /** Drained by the Rust accept-poller via accepted_connections(). */
        val sharedAcceptedConnections = ConcurrentLinkedQueue<FfiAcceptedConnection>()

        /** Drained by the Rust inbound reassembly poller via incoming_gatt_writes(). */
        val sharedGattWrites = ConcurrentLinkedQueue<FfiGattWriteEvent>()

        /**
         * HV-21/DEC-BLE-0008: bytes served on a read of
         * [IRIS_IDENTIFY_CHARACTERISTIC_UUID] — set via [setIdentifyPayload],
         * called from Rust's `ble.rs::start_advertising` (and again from
         * `set_local_wifi_direct_mac`) with this node's own beacon, rebuilt
         * with the real Wi-Fi Direct MAC folded in once known. Process-wide
         * like the GATT server itself (HV-109) — a remote central's read can
         * land on any adapter instance's shared server.
         */
        @Volatile
        private var sharedIdentifyPayload: ByteArray = ByteArray(0)

        /** AND-RT-113: collision-resistant per-device FFI `handle` for server writes. */
        fun deviceHash(device: BluetoothDevice): ULong {
            val address = device.address.replace(":", "").lowercase()
            val digest = MessageDigest.getInstance("SHA-256")
                .digest(address.toByteArray(Charsets.US_ASCII))
            return (ByteBuffer.wrap(digest).long and Long.MAX_VALUE).toULong()
        }

        private val sharedGattServerCallback = object : BluetoothGattServerCallback() {
            override fun onConnectionStateChange(device: BluetoothDevice, status: Int, newState: Int) {
                android.util.Log.d(
                    "IrisBleDiag",
                    "gattServer onConnectionStateChange device=${device.address} status=$status newState=$newState",
                )
                if (newState == BluetoothProfile.STATE_CONNECTED) {
                    while (sharedAcceptedConnections.size >= MAX_PENDING) sharedAcceptedConnections.poll()
                    sharedAcceptedConnections.add(
                        FfiAcceptedConnection(handle = deviceHash(device), address = device.address),
                    )
                }
            }

            override fun onCharacteristicWriteRequest(
                device: BluetoothDevice,
                requestId: Int,
                characteristic: BluetoothGattCharacteristic,
                preparedWrite: Boolean,
                responseNeeded: Boolean,
                offset: Int,
                value: ByteArray,
            ) {
                android.util.Log.d(
                    "IrisBleDiag",
                    "onCharacteristicWriteRequest device=${device.address} uuid=${characteristic.uuid} " +
                        "matchesIris=${characteristic.uuid == IRIS_CHARACTERISTIC_UUID} len=${value.size} " +
                        "responseNeeded=$responseNeeded",
                )
                if (characteristic.uuid == IRIS_CHARACTERISTIC_UUID) {
                    // HV-93/HV-98: on several OEM stacks the GATT server never
                    // fires onConnectionStateChange for an inbound LE link, so
                    // the write itself is the only reliable "this peer is
                    // talking to us" signal — announce on EVERY IRIS write. The
                    // Rust accept-poller de-dups (`accept_spawned`).
                    val h = deviceHash(device)
                    while (sharedAcceptedConnections.size >= MAX_PENDING) sharedAcceptedConnections.poll()
                    sharedAcceptedConnections.add(FfiAcceptedConnection(handle = h, address = device.address))
                    while (sharedGattWrites.size >= MAX_PENDING) sharedGattWrites.poll()
                    sharedGattWrites.add(
                        FfiGattWriteEvent(
                            handle = h,
                            charUuid = characteristic.uuid.toString(),
                            data = value,
                        ),
                    )
                }
                if (responseNeeded) {
                    try {
                        sharedGattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, null)
                    } catch (_: Throwable) {
                    }
                }
            }

            override fun onCharacteristicReadRequest(
                device: BluetoothDevice,
                requestId: Int,
                offset: Int,
                characteristic: BluetoothGattCharacteristic,
            ) {
                // HV-21/DEC-BLE-0008: only IRIS_IDENTIFY_CHARACTERISTIC_UUID
                // is ever registered as readable (see ensureSharedGattServer)
                // — the UUID check here is defensive, matching the pattern
                // the write handler above already uses.
                val value = if (characteristic.uuid == IRIS_IDENTIFY_CHARACTERISTIC_UUID) {
                    sharedIdentifyPayload
                } else {
                    ByteArray(0)
                }
                try {
                    sharedGattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, value)
                } catch (_: Throwable) {
                }
            }
        }

        /**
         * Open the one process-wide GATT server (once) and serve the IRIS
         * transport characteristic. Idempotent; never closed on mesh-stop.
         * HV-95: bounded retry for a wedged OEM stack.
         */
        fun ensureSharedGattServer(bleManager: BluetoothManager?, appContext: Context): Boolean {
            if (sharedGattServer != null) return true
            synchronized(this) {
                if (sharedGattServer != null) return true
                repeat(3) { attempt ->
                    val server = bleManager?.openGattServer(appContext, sharedGattServerCallback)
                    if (server == null) {
                        iriscore.util.IrisLog.w("ble.gatt", "ensureSharedGattServer: openGattServer null (attempt ${attempt + 1}/3)")
                        Thread.sleep(150L)
                        return@repeat
                    }
                    val service = BluetoothGattService(IRIS_SERVICE_UUID, BluetoothGattService.SERVICE_TYPE_PRIMARY)
                    service.addCharacteristic(
                        BluetoothGattCharacteristic(
                            IRIS_CHARACTERISTIC_UUID,
                            BluetoothGattCharacteristic.PROPERTY_WRITE or
                                BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE or
                                BluetoothGattCharacteristic.PROPERTY_NOTIFY,
                            BluetoothGattCharacteristic.PERMISSION_WRITE,
                        ),
                    )
                    // HV-21/DEC-BLE-0008: read-only identify characteristic —
                    // see IRIS_IDENTIFY_CHARACTERISTIC_UUID's doc.
                    service.addCharacteristic(
                        BluetoothGattCharacteristic(
                            IRIS_IDENTIFY_CHARACTERISTIC_UUID,
                            BluetoothGattCharacteristic.PROPERTY_READ,
                            BluetoothGattCharacteristic.PERMISSION_READ,
                        ),
                    )
                    val added = server.addService(service)
                    iriscore.util.IrisLog.d("ble.gatt", "ensureSharedGattServer: addService -> $added")
                    if (added) {
                        sharedGattServer = server
                        return true
                    }
                    try { server.close() } catch (_: Throwable) {}
                    Thread.sleep(150L)
                }
                iriscore.util.IrisLog.w("ble.gatt", "ensureSharedGattServer: no server after 3 attempts — stack may be wedged")
                return false
            }
        }

        /** HV-109: only on a real BT-off (the OS has already invalidated it). */
        fun closeSharedGattServer() {
            synchronized(this) {
                try { sharedGattServer?.close() } catch (_: Throwable) {}
                sharedGattServer = null
                sharedGattWrites.clear()
                sharedAcceptedConnections.clear()
            }
        }
    }

    private val appContext: Context = context.applicationContext
    /**
     * Null when the device has no Bluetooth hardware. The cast used to be
     * non-null (`as BluetoothManager`), which threw at Hilt injection time on
     * the main thread — the app died before it could report anything.
     */
    private val bleManager: BluetoothManager? =
        appContext.getSystemService(Context.BLUETOOTH_SERVICE) as? BluetoothManager

    // Handle tables (handle -> live platform object).
    private val nextHandle = AtomicLong(1L)
    private val scanHandles = ConcurrentHashMap<Long, BluetoothLeScanner>()
    private val gattHandles = ConcurrentHashMap<Long, BluetoothGatt>()

    // Drain buffers (proj-BLE-1 — owned projection of the core drains).
    // ConcurrentLinkedQueue, not CopyOnWriteArrayList: the drain is a
    // poll-until-empty loop (atomic per item), and every COW `add` copied the
    // whole backing array — O(n^2) churn precisely when the core has stopped
    // draining and the buffer is growing.
    private val pendingScanResults = ConcurrentLinkedQueue<FfiScanResult>()

    // HV-109: the GATT-server inbound queues are process-wide (companion) so a
    // central's writes route no matter which adapter instance is currently live.
    private val pendingGattWrites get() = sharedGattWrites
    private val pendingAcceptedConnections get() = sharedAcceptedConnections

    // AND-RT-111: IRIS characteristic cached by onServicesDiscovered (async);
    // negotiated MTU + last client-write status tracked per connection.
    private val serviceCharacteristics = ConcurrentHashMap<BluetoothGatt, BluetoothGattCharacteristic>()
    // HV-92: per-connection count of service-discovery passes that came back
    // without the IRIS service, so onServicesDiscovered can retry a bounded number
    // of times before failing the connectGatt() waiter.
    private val serviceDiscoveryRetries = ConcurrentHashMap<BluetoothGatt, Int>()
    private val negotiatedMtu = ConcurrentHashMap<BluetoothGatt, Int>()
    private val gattWriteFailures = ConcurrentHashMap<BluetoothGatt, Int>()

    /**
     * HW-7: Android permits only one outstanding GATT operation per
     * connection — nothing queues this for you (confirmed against
     * bitchat-android's own BluetoothPacketBroadcaster, which serializes
     * writes with exactly this pattern: a per-link in-flight flag cleared
     * only by the completion callback, never a blind retry). `gattWrite()`
     * used to return as soon as `writeCharacteristic()` was *initiated*
     * (the synchronous call returning success), not once it actually
     * *completed* (`onCharacteristicWrite`, asynchronous) — so the very
     * next GATT touch on the same connection (the next fragment of the
     * same message, MTU renegotiation, anything) could race a write the
     * controller hadn't finished yet, and Android correctly refused it.
     * HW-6's bounded retry papered over the symptom without addressing
     * this; confirmed live it wasn't sufficient on its own. One pending
     * completion future per live `BluetoothGatt`, resolved from
     * `onCharacteristicWrite`, makes `gattWrite()` block until the write
     * is genuinely done — mirrors `connectionReady`'s existing pattern
     * for `connectGatt`/service discovery below.
     */
    private val writeCompletion =
        ConcurrentHashMap<BluetoothGatt, java.util.concurrent.CompletableFuture<Unit>>()

    // Scan-restart throttle (adapter side; 30-s rolling window).
    private val scanStartTimes = ConcurrentSkipListSet<Long>()

    private val scanCallback = object : ScanCallback() {
        override fun onScanResult(callbackType: Int, result: AndroidScanResult) {
            if (result.rssi < rssiFloor) return
            val device = result.device ?: return
            // The IRIS beacon (DiscoveryBeacon::parse) is the bytes advertised
            // under Manufacturer Specific Data (see startAdvertising's
            // addManufacturerData) — NOT the raw scan record, and not Service
            // Data (a 128-bit service UUID doesn't fit the beacon in legacy
            // advertising's 31-byte budget; see IRIS_MANUFACTURER_ID).
            // scanRecord.bytes is every AD structure concatenated with TLV
            // length/type headers, so reading it directly fed DiscoveryBeacon
            // ::parse() garbage that always failed UnsupportedVersion.
            val bytes = result.scanRecord?.getManufacturerSpecificData(IRIS_MANUFACTURER_ID) ?: ByteArray(0)
            while (pendingScanResults.size >= MAX_PENDING) pendingScanResults.poll()
            pendingScanResults.add(
                FfiScanResult(
                    address = device.address,
                    payload = bytes,
                    rssi = result.rssi,
                ),
            )
        }

        override fun onScanFailed(errorCode: Int) {
            // HV-11: this used to be log-only, so the core's scan_allowed()
            // backoff never learned the platform refused. Queue the code for
            // drainScanFailures() — BleTransport::discover_peers re-arms a scan
            // the OS silently rejected (SCAN_FAILED_APPLICATION_REGISTRATION_
            // FAILED=2, INTERNAL_ERROR=3, SCANNING_TOO_FREQUENTLY=6).
            iriscore.util.IrisLog.w("ble.scan", "startScan failed errorCode=$errorCode")
            while (scanFailures.size >= MAX_PENDING) scanFailures.poll()
            scanFailures.add(errorCode)
        }
    }

    /** HV-11: onScanFailed error codes awaiting drainScanFailures(). */
    private val scanFailures = ConcurrentLinkedQueue<Int>()

    /** HV-94: client GATT handles whose link dropped, awaiting drainDisconnectedHandles(). */
    private val pendingDisconnectedHandles = ConcurrentLinkedQueue<Long>()

    /** HV-10/HV-31: adapter-lifecycle event codes awaiting drainAdapterEvents(). */
    private val adapterEvents = ConcurrentLinkedQueue<Int>()

    /**
     * HV-10/HV-31: off-thread worker for advertising-retry backoff and for
     * replaying advertise+scan after a Bluetooth off→on toggle — a
     * `BroadcastReceiver.onReceive` runs on the main thread and must not make
     * blocking BLE calls.
     */
    private val recovery: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor { r ->
            Thread(r, "iris-ble-recovery").apply { isDaemon = true }
        }

    /** HV-10: per-advertise-handle count of `onStartFailure` retries so far. */
    private val advertiseRetries = ConcurrentHashMap<Long, Int>()

    /** HV-31: guards a single registration of [btStateReceiver]. */
    private val btReceiverRegistered = AtomicBoolean(false)

    /** HV-27: guards a single start of the stale-link liveness tick. */
    private val livenessTickStarted = AtomicBoolean(false)

    /**
     * HV-27: a silently-dead GATT link — peer walked out of range, RF glitch —
     * should fire `onConnectionStateChange(DISCONNECTED)` once Android's ~20 s
     * link-supervision timeout expires, but on the OEM stacks we target that
     * callback is unreliable (HV-93/HV-94). Cross-check our live client handles
     * against the stack's own connected-device list every few seconds; any
     * handle the stack no longer lists is dead — feed it to the same
     * `pendingDisconnectedHandles` drain HV-94 wired, so the Rust accept-poller
     * tears the peer down and discovery reconnects, instead of the link sitting
     * dead until the user's next send fails.
     */
    private fun ensureLivenessTick() {
        if (!livenessTickStarted.compareAndSet(false, true)) return
        recovery.scheduleWithFixedDelay({
            try {
                val mgr = bleManager ?: return@scheduleWithFixedDelay
                if (gattHandles.isEmpty()) return@scheduleWithFixedDelay
                val live = mgr.getConnectedDevices(BluetoothProfile.GATT).map { it.address }.toHashSet()
                val stale = gattHandles.entries.filter { it.value.device.address !in live }
                for (e in stale) {
                    iriscore.util.IrisLog.w("ble.gatt", "liveness: handle ${e.key} no longer stack-connected — treating as disconnected")
                    gattHandles.remove(e.key)
                    serviceCharacteristics.remove(e.value)
                    negotiatedMtu.remove(e.value)
                    quietly { e.value.close() }
                    while (pendingDisconnectedHandles.size >= MAX_PENDING) pendingDisconnectedHandles.poll()
                    pendingDisconnectedHandles.add(e.key)
                }
            } catch (t: Throwable) {
                iriscore.util.IrisLog.w("ble.gatt", "liveness tick threw: ${t.message}")
            }
        }, LIVENESS_TICK_MS, LIVENESS_TICK_MS, TimeUnit.MILLISECONDS)
    }

    private fun enqueueAdapterEvent(code: Int) {
        while (adapterEvents.size >= MAX_PENDING) adapterEvents.poll()
        adapterEvents.add(code)
    }

    /**
     * RSSI floor applied on the platform side. HV-12: defaults to the same
     * -95 dBm as `iris_core::transport::ble::RSSI_FLOOR_DBM` (was -127 =
     * "accept everything", which is the floor that actually applied on any
     * scan the OS refused, or before the first `startScan`). A `startScan`
     * overwrites this with `filter.rssiFloor`, which the bridge also derives
     * from `RSSI_FLOOR_DBM`.
     */
    @Volatile
    private var rssiFloor: Int = -95

    private val gattCallback = object : BluetoothGattCallback() {
        override fun onCharacteristicWrite(
            gatt: BluetoothGatt,
            characteristic: BluetoothGattCharacteristic,
            status: Int,
        ) {
            // AND-RT-111: surface failed client writes — a later gattWrite to the
            // same connection surfaces them as a typed error instead of silence.
            if (status == BluetoothGatt.GATT_SUCCESS) {
                gattWriteFailures.remove(gatt)
            } else {
                gattWriteFailures[gatt] = status
            }
            // HW-7: this is what gattWrite()'s writeCompletion future is
            // actually waiting on — resolving it here is what makes the
            // write genuinely synchronous-and-complete from the caller's
            // perspective, not just "initiated."
            val pending = writeCompletion.remove(gatt)
            if (status == BluetoothGatt.GATT_SUCCESS) {
                pending?.complete(Unit)
            } else {
                pending?.completeExceptionally(GattFailure("gatt write failed status=$status"))
            }
        }

        override fun onMtuChanged(gatt: BluetoothGatt, mtu: Int, status: Int) {
            // AND-RT-111: track the negotiated MTU (setMtu returns it).
            if (status == BluetoothGatt.GATT_SUCCESS) negotiatedMtu[gatt] = mtu
        }

        override fun onServicesDiscovered(gatt: BluetoothGatt, status: Int) {
            // AND-RT-111: cache the IRIS characteristic so gattWrite can resolve
            // it without a full re-scan of gatt.services.
            if (status != BluetoothGatt.GATT_SUCCESS) {
                // BLE-2: service discovery is part of what connectGatt() now
                // waits for (see connectionReady) — a failure here must wake
                // that waiter with an error, not leave it blocked until the
                // outer FFI watchdog times out 30s later.
                connectionReady.remove(gatt)
                    ?.completeExceptionally(GattFailure("service discovery failed status=$status"))
                gatt.disconnect()
                gatt.close()
                return
            }
            val characteristic =
                gatt.getService(IRIS_SERVICE_UUID)?.getCharacteristic(IRIS_CHARACTERISTIC_UUID)
            if (characteristic != null) {
                serviceCharacteristics[gatt] = characteristic
                serviceDiscoveryRetries.remove(gatt)
                // Connection is genuinely usable now: STATE_CONNECTED happened
                // AND the characteristic gattWrite needs is resolvable. This is
                // what connectGatt() has been blocking on.
                connectionReady.remove(gatt)?.complete(Unit)
                return
            }
            // HV-92: GATT_SUCCESS but the IRIS service is not in the discovered
            // database yet (Android's incomplete first-discovery). Do NOT report
            // the link ready — Rust would then store a connection whose very
            // first gattWrite fails "characteristic not yet discovered" and the
            // message is dropped. Re-run discovery a bounded number of times.
            val tries = (serviceDiscoveryRetries[gatt] ?: 0) + 1
            serviceDiscoveryRetries[gatt] = tries
            if (tries <= SERVICE_DISCOVERY_RETRY_ATTEMPTS) {
                iriscore.util.IrisLog.w(
                    "ble.gatt",
                    "onServicesDiscovered: IRIS characteristic absent, re-discovering ($tries/$SERVICE_DISCOVERY_RETRY_ATTEMPTS)",
                )
                Thread({
                    try {
                        Thread.sleep(SERVICE_DISCOVERY_RETRY_DELAY_MS)
                    } catch (_: InterruptedException) {
                    }
                    try {
                        if (!gatt.discoverServices()) {
                            serviceDiscoveryRetries.remove(gatt)
                            connectionReady.remove(gatt)?.completeExceptionally(
                                GattFailure("characteristic not yet discovered (re-discovery refused)"),
                            )
                        }
                    } catch (e: SecurityException) {
                        serviceDiscoveryRetries.remove(gatt)
                        connectionReady.remove(gatt)?.completeExceptionally(e)
                    }
                }, "iris-ble-rediscover").apply { isDaemon = true }.start()
                return
            }
            serviceDiscoveryRetries.remove(gatt)
            connectionReady.remove(gatt)?.completeExceptionally(
                GattFailure(
                    "characteristic not yet discovered after $SERVICE_DISCOVERY_RETRY_ATTEMPTS discovery attempts",
                ),
            )
            gatt.disconnect()
            gatt.close()
        }

        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            if (newState == BluetoothGatt.STATE_CONNECTED) {
                // Android never discovers services on its own; without this call
                // gatt.services stays empty forever, onServicesDiscovered never
                // fires, serviceCharacteristics never gets populated, and every
                // gattWrite() on this connection fails with "characteristic not
                // yet discovered" — the connection looks live (connect_gatt()
                // already returned Ok) but can never actually carry a message.
                // This callback runs off the FFI call stack (async platform
                // event), so a permission loss here has nowhere typed to go;
                // wake the connectGatt() waiter with an error instead of
                // leaving it to time out, then fall through to the normal
                // teardown-on-failure path.
                try {
                    gatt.discoverServices()
                } catch (e: SecurityException) {
                    connectionReady.remove(gatt)?.completeExceptionally(e)
                }
            } else if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                // HV-94: surface the client-side link-down to the Rust core the
                // same way HW-9 surfaces server-side accepts — a drain the
                // accept-poller consumes. Without this, a peer whose *app
                // process* died (its GATT server gone, ACL dropped) is only
                // noticed when the next `send()` write fails with "unknown gatt
                // connection" — one message is lost and `close_peer` runs late.
                val removed = gattHandles.entries.filter { it.value == gatt }.map { it.key }
                gattHandles.entries.removeIf { it.value == gatt }
                for (h in removed) {
                    while (pendingDisconnectedHandles.size >= MAX_PENDING) pendingDisconnectedHandles.poll()
                    pendingDisconnectedHandles.add(h)
                }
                serviceCharacteristics.remove(gatt)
                serviceDiscoveryRetries.remove(gatt)
                negotiatedMtu.remove(gatt)
                gattWriteFailures.remove(gatt)
                // HV-33: fail any in-flight write on this connection now, with
                // the numeric GATT status, instead of letting it burn the full
                // GATT_WRITE_TIMEOUT_MS. The core classifies the code (133/8/62/
                // 22/19 → transient hold + reconnect; else Protocol).
                writeCompletion.remove(gatt)
                    ?.completeExceptionally(GattFailure("gatt write failed status=$status (link dropped)"))
                // BLE-2: a disconnect before onServicesDiscovered ever ran
                // means the connect attempt failed outright (refused, out of
                // range, ACL timeout, ...) — surface that to connectGatt()'s
                // waiter immediately rather than blocking it for the full
                // FFI watchdog timeout only to fail anyway.
                connectionReady.remove(gatt)
                    ?.completeExceptionally(GattFailure("disconnected before connect completed, status=$status"))
                gatt.close()
            }
        }
    }

    /**
     * BLE-2: resolved by [gattCallback] once a connection initiated by
     * [connectGatt] is genuinely usable — `STATE_CONNECTED` AND service
     * discovery both completed — or fails outright. `connectGatt()` blocks
     * on the matching entry so its FFI contract (a synchronous return)
     * means "this peer can actually be written to now", which it did not
     * before: `device.connectGatt()` only *initiates* the platform's ACL
     * handshake, and the old code returned the instant that call was made,
     * long before Android confirmed anything. Rust believed the peer was
     * connected and immediately tried to write to it — before the real
     * handshake had even finished — so the very first write always failed
     * ("characteristic not yet discovered") and tore the brand-new
     * connection back down (BLE-1) before it was ever usable. Every
     * subsequent discovery cycle repeated the identical race: no message
     * could ever be delivered on real hardware.
     */
    private val connectionReady =
        ConcurrentHashMap<BluetoothGatt, java.util.concurrent.CompletableFuture<Unit>>()

    /**
     * Peripheral-side GATT server (ANDROID.md §BLE: GATT server write/notify
     * characteristics). Peer centrals write frames to [IRIS_CHARACTERISTIC_UUID];
     * those writes are DRAINed by the core bridge via `incomingGattWrites()`
     * (proj-BLE-1 — never a MutexGuard across FFI).
     */
    // HV-109: the GATT server, its callback, its inbound queues and `deviceHash`
    // all live in the companion object now — one server for the life of the
    // process, shared by every adapter instance the snippet/app builds. See the
    // companion block for the full rationale.
    private fun ensureGattServer(): Boolean =
        ensureSharedGattServer(bleManager, appContext)

    private val advertiseHandles = ConcurrentHashMap<Long, Pair<BluetoothLeAdvertiser, AdvertiseCallback>>()

    override fun startScan(filter: FfiScanFilter): ULong {
        // Was `syncCall(onTimeout = throwTimeout())`: throwTimeout() throws
        // eagerly while Kotlin builds syncCall's argument list, before the
        // block below - the real scan start - ever runs. Every call to
        // startScan threw Timeout unconditionally. syncCallOrThrow defers
        // the throw to where it belongs (AdapterLifecycle.kt).
        return FfiCallTimeout.syncCallOrThrow {
            // A handle of 0 used to be returned for "no scanner", "throttled"
            // and "timed out". Rust wrapped that as Ok(ScanHandle(0)), so the
            // core believed a scan was live when nothing had started and never
            // retried. These are now typed failures.
            val scanner = bleManager?.adapter?.bluetoothLeScanner
                ?: throw AdapterOff()
            val now = SystemClock.elapsedRealtime()
            pruneOldStarts(now)
            if (scanStartTimes.size >= SCAN_RESTART_CEILING) {
                // Reported, not silently swallowed — the core's own backoff is
                // authoritative and needs to know the start was refused.
                throw Transport("BLE scan restart ceiling reached")
            }
            // Only mutate the shared filter once the start is actually going
            // ahead; a refused call used to leave rssiFloor changed globally.
            rssiFloor = filter.rssiFloor
            scanStartTimes.add(now)
            // A revoked BLUETOOTH_SCAN throws SecurityException; map it to the
            // declared FFI error rather than letting it abort the Rust side.

            val filters = buildScanFilters(filter)
            permitted { scanner.startScan(filters, scanSettings(), scanCallback) }
            val handle = nextHandle.getAndIncrement()
            scanHandles[handle] = scanner
            handle.toULong()
        }
    }

    override fun stopScan(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            // `stop_scan` has no error type in the FFI contract, so anything
            // thrown here becomes UNIFFI_CALL_UNEXPECTED_ERROR on a method with
            // no error channel — which aborts the process. A user revoking
            // BLUETOOTH_SCAN from Settings mid-session did exactly that.
            quietly { scanHandles.remove(handle.toLong())?.stopScan(scanCallback) }
        }
    }

    override fun startAdvertising(data: FfiAdvertisementData): ULong {
        // Same defect as startScan above: syncCall(onTimeout = throwTimeout())
        // threw Timeout unconditionally, before advertising was ever
        // attempted. Confirmed on a real device (API 34): every call failed
        // immediately with "transport protocol error: timeout", under 2s,
        // nowhere near the 30s budget - because the real operation never ran.
        return FfiCallTimeout.syncCallOrThrow {
            val advertiser = bleManager?.adapter?.bluetoothLeAdvertiser
                ?: throw AdapterOff()
            // HV-95: do NOT advertise a connectable beacon we cannot serve.
            if (!ensureGattServer()) {
                throw Transport("BLE GATT server unavailable — the Bluetooth stack may be wedged; toggle Bluetooth")
            }
            val settings = AdvertiseSettings.Builder()
                // HV-91: LOW_POWER advertises at a ~1 s interval, so a scanning
                // peer can take many seconds to catch the first IRIS beacon.
                // BALANCED (~250 ms) roughly quarters that with a modest power
                // cost — the app only advertises while its foreground mesh
                // service is up. HV-80 will make this adaptive.
                .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_BALANCED)
                .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM)
                .setConnectable(!data.nonConnectable)
                .build()
            // Manufacturer Specific Data (2-byte company id), not Service Data
            // (would need the 16-byte IRIS_SERVICE_UUID on air) — see
            // IRIS_MANUFACTURER_ID for why: the beacon plus a 128-bit UUID
            // cannot fit legacy advertising's 31-byte budget on any device,
            // and most real hardware (confirmed on one of the two physical
            // test devices) has no BLE 5 extended-advertising fallback.
            val adData = AdvertiseData.Builder()
                .addManufacturerData(IRIS_MANUFACTURER_ID, data.payload)
                .build()
            if (adData.manufacturerSpecificData.size() == 0) {
                throw InvalidArgument("advertisement payload produced no manufacturer data")
            }
            val handle = nextHandle.getAndIncrement()
            advertiseRetries.remove(handle)
            ensureBtStateReceiver()
            launchAdvertising(handle, settings, adData)
            handle.toULong()
        }
    }

    /**
     * HV-10/HV-31: (re)start one advertising set under [handle], installing an
     * [AdvertiseCallback] whose `onStartFailure` schedules a bounded
     * exponential-backoff retry for the transient error codes and, once those
     * are exhausted, reports [EVT_ADVERTISING_GAVE_UP] to the core. Called from
     * `startAdvertising`, from the retry timer, and from [btStateReceiver] on
     * `STATE_ON`.
     */
    private fun launchAdvertising(
        handle: Long,
        settings: AdvertiseSettings,
        adData: AdvertiseData,
    ) {
        val advertiser = bleManager?.adapter?.bluetoothLeAdvertiser ?: return
        // Always stop any prior set on this handle first — required for
        // TOO_MANY_ADVERTISERS recovery (a leaked set must be released before
        // the OS will accept a new one).
        quietly {
            advertiseHandles.remove(handle)?.let { (a, cb) -> a.stopAdvertising(cb) }
        }
        val callback = object : AdvertiseCallback() {
            override fun onStartSuccess(settingsInEffect: AdvertiseSettings?) {
                advertiseRetries.remove(handle)
            }

            override fun onStartFailure(errorCode: Int) {
                iriscore.util.IrisLog.w("ble.advert", "startAdvertising failed errorCode=$errorCode")
                advertiseHandles.remove(handle)
                // 1 DATA_TOO_LARGE / 5 FEATURE_UNSUPPORTED are terminal.
                val retryable = errorCode == AdvertiseCallback.ADVERTISE_FAILED_TOO_MANY_ADVERTISERS ||
                    errorCode == AdvertiseCallback.ADVERTISE_FAILED_ALREADY_STARTED ||
                    errorCode == AdvertiseCallback.ADVERTISE_FAILED_INTERNAL_ERROR
                val attempt = advertiseRetries.getOrDefault(handle, 0)
                if (retryable && attempt < ADVERTISE_RETRY_DELAYS_MS.size) {
                    advertiseRetries[handle] = attempt + 1
                    val delay = ADVERTISE_RETRY_DELAYS_MS[attempt]
                    iriscore.util.IrisLog.w(
                        "ble.advert",
                        "scheduling advertising retry ${attempt + 1}/${ADVERTISE_RETRY_DELAYS_MS.size} in ${delay}ms",
                    )
                    quietly {
                        recovery.schedule(
                            { launchAdvertising(handle, settings, adData) },
                            delay,
                            TimeUnit.MILLISECONDS,
                        )
                    }
                } else {
                    iriscore.util.IrisLog.w(
                        "ble.advert",
                        "advertising retries exhausted (errorCode=$errorCode) — reporting to core",
                    )
                    enqueueAdapterEvent(EVT_ADVERTISING_GAVE_UP)
                }
            }
        }
        advertiseHandles[handle] = advertiser to callback
        try {
            permitted { advertiser.startAdvertising(settings, adData, callback) }
        } catch (e: Exception) {
            advertiseHandles.remove(handle)
            iriscore.util.IrisLog.w("ble.advert", "startAdvertising threw: ${e.message}")
        }
    }

    override fun stopAdvertising(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            quietly {
                advertiseHandles.remove(handle.toLong())?.let { (advertiser, callback) ->
                    advertiser.stopAdvertising(callback)
                }
            }
            // HV-109: do NOT close the GATT server here. It is a process
            // resource, shared across every adapter instance (companion
            // `sharedGattServer`); a remote central's connection to it survives
            // a peripheral-side `close()` anyway (Bluetooth core spec), so
            // closing it only stranded that central against a database it
            // couldn't re-discover. HV-97's "second stale server" is now
            // impossible — there is only ever one. Only the advertiser stops.
            if (advertiseHandles.isEmpty()) {
                // HV-10/HV-31: this transport is no longer advertising — drop
                // the toggle receiver so a later STATE_ON does not try to
                // resurrect an advertisement the core stopped.
                advertiseRetries.clear()
                if (btReceiverRegistered.compareAndSet(true, false)) {
                    quietly { appContext.unregisterReceiver(btStateReceiver) }
                }
            }
        }
    }

    /**
     * HV-31: a Bluetooth off→on toggle (or airplane mode) invalidates every
     * scanner/advertiser/GATT-server handle at the OS level and fires nothing
     * on our callbacks. The Wi-Fi Direct adapter has always listened for
     * `WIFI_P2P_STATE_CHANGED`; the BLE adapter never did. On `STATE_OFF` drop
     * the dead handles and tell the core (`EVT_BLUETOOTH_OFF` → transport
     * Unavailable); on `STATE_ON` report `EVT_BLUETOOTH_ON` — the core re-drives
     * `startAdvertising` (which reopens the GATT server) and forces a scan
     * re-arm on its next discovery pass. Replaying from the core rather than
     * inside `onReceive` keeps blocking BLE calls off the main thread and off a
     * freshly-restarted, still-settling stack.
     */
    private val btStateReceiver = object : BroadcastReceiver() {
        override fun onReceive(context: Context, intent: Intent) {
            if (intent.action != BluetoothAdapter.ACTION_STATE_CHANGED) return
            when (intent.getIntExtra(BluetoothAdapter.EXTRA_STATE, BluetoothAdapter.ERROR)) {
                BluetoothAdapter.STATE_OFF, BluetoothAdapter.STATE_TURNING_OFF -> {
                    iriscore.util.IrisLog.w("ble.adapter", "Bluetooth OFF — dropping stale BLE handles")
                    // HV-109: the OS invalidates the GATT server on a real BT-off,
                    // so drop the shared one here — `ensureSharedGattServer`
                    // reopens it when the core re-advertises after BT-on.
                    closeSharedGattServer()
                    scanHandles.clear()
                    advertiseHandles.clear()
                    scanStartTimes.clear()
                    serviceCharacteristics.clear()
                    serviceDiscoveryRetries.clear()
                    negotiatedMtu.clear()
                    gattWriteFailures.clear()
                    connectionReady.clear()
                    writeCompletion.clear()
                    enqueueAdapterEvent(EVT_BLUETOOTH_OFF)
                }
                BluetoothAdapter.STATE_ON -> {
                    iriscore.util.IrisLog.i("ble.adapter", "Bluetooth ON — core will re-advertise + re-scan")
                    enqueueAdapterEvent(EVT_BLUETOOTH_ON)
                }
            }
        }
    }

    private fun ensureBtStateReceiver() {
        if (!btReceiverRegistered.compareAndSet(false, true)) return
        quietly {
            ContextCompat.registerReceiver(
                appContext,
                btStateReceiver,
                IntentFilter(BluetoothAdapter.ACTION_STATE_CHANGED),
                ContextCompat.RECEIVER_NOT_EXPORTED,
            )
        }
    }

    override fun connectGatt(address: String): ULong {
        // Was `syncCall(onTimeout = 0uL)`: on a real overrun (device out of
        // range, ACL handshake stuck, ...) that silently returned handle 0
        // as if it were a genuine connection — Rust had no way to tell a
        // timed-out connect from a successful one, so it proceeded straight
        // to gattWrite() against a handle nothing was ever registered
        // under. syncCallOrThrow surfaces the overrun as Timeout instead.
        return FfiCallTimeout.syncCallOrThrow(timeoutMs = CONNECT_READY_TIMEOUT_MS + 2_000L) {
            val adapter = bleManager?.adapter ?: throw AdapterOff()
            // The Rust bridge emits a bare 12-hex address ("AABBCCDDEEFF"), but
            // getRemoteDevice demands "AA:BB:CC:DD:EE:FF" and throws
            // IllegalArgumentException otherwise — an untyped exception that
            // became UNIFFI_CALL_UNEXPECTED_ERROR. BLE outbound connection could
            // never succeed. Normalise here and reject cleanly.
            val mac = toBluetoothMac(address)
                ?: throw InvalidArgument("malformed BLE address: $address")
            val device: BluetoothDevice = adapter.getRemoteDevice(mac)
            // Positional args: connectGatt is a Java method, so Kotlin named
            // arguments are not available for it.
            //
            // HV-101: `autoConnect = false` is correct for every IRIS connect —
            // discovery-driven reconnects are foreground/mesh-critical, and
            // `autoConnect = true` is a power-optimised BACKGROUND path, not a
            // faster one (bleadvertiserapp.medium.com "Android 15 Broke BLE
            // Reconnection Speed"; the same source: re-bonding does not help
            // reconnect latency either — HV-34). What the Android-14/15
            // reconnect-stall fix actually needs is (a) an explicit transport,
            // (b) a bounded connect watchdog, (c) exponential connect backoff,
            // (d) status-133-is-transient handling — IRIS already has (b) via
            // `connectionReady` + `CONNECT_READY_TIMEOUT_MS`, (c) via the core
            // `DiscoveryManager` `connect_backoff` (5→10→20→40→80 s), and (d)
            // via HV-33. (a) is this: pin `TRANSPORT_LE` so a dual-mode peer's
            // stack never wastes the budget probing BR/EDR first.
            val gatt = permitted {
                device.connectGatt(appContext, false, gattCallback, BluetoothDevice.TRANSPORT_LE)
            } ?: throw DeviceNotFound()
            iriscore.util.IrisLog.d("ble.gatt", "connectGatt: initiated (autoConnect=false, TRANSPORT_LE), waiting for ready")
            val ready = java.util.concurrent.CompletableFuture<Unit>()
            connectionReady[gatt] = ready
            // BLE-2: block this watchdog-pool thread (never the main/Binder
            // thread — see FfiCallTimeout) until gattCallback resolves
            // [ready]: STATE_CONNECTED *and* service discovery both
            // completed, or a failure. The outer syncCall's own timeout
            // (30s) is the backstop if neither ever arrives.
            try {
                // HV-99: bounded — do not inherit Android's ~30 s direct-connect
                // stack timeout for a discovery reconnect probe.
                ready.get(CONNECT_READY_TIMEOUT_MS, java.util.concurrent.TimeUnit.MILLISECONDS)
                iriscore.util.IrisLog.d("ble.gatt", "connectGatt: ready resolved successfully")
            } catch (e: java.util.concurrent.TimeoutException) {
                iriscore.util.IrisLog.w("ble.gatt", "connectGatt: not ready within ${CONNECT_READY_TIMEOUT_MS}ms — aborting probe")
                connectionReady.remove(gatt)
                quietly { gatt.disconnect(); gatt.close() }
                throw Timeout()
            } catch (e: java.util.concurrent.ExecutionException) {
                iriscore.util.IrisLog.w("ble.gatt", "connectGatt: ready failed", e)
                connectionReady.remove(gatt)
                quietly { gatt.close() }
                throw (e.cause as? Exception) ?: GattFailure("connect failed: ${e.cause}")
            }
            val handle = nextHandle.getAndIncrement()
            gattHandles[handle] = gatt
            ensureLivenessTick() // HV-27
            handle.toULong()
        }
    }

    override fun disconnectGatt(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            quietly {
                gattHandles.remove(handle.toLong())?.let { gatt ->
                    gatt.disconnect()
                    gatt.close()
                }
            }
        }
    }

    override fun gattWrite(handle: ULong, charUuid: String, data: ByteArray) {
        FfiCallTimeout.syncCallOrThrow(timeoutMs = GATT_WRITE_TIMEOUT_MS + 1000L) {
            iriscore.util.IrisLog.d("ble.gatt", "gattWrite ENTER handle=$handle len=${data.size} known=${gattHandles.keys}")
            val gatt = gattHandles[handle.toLong()]
                ?: throw GattFailure("unknown gatt connection")
            // AND-RT-111: a write before discovery resolves to a typed error,
            // never a silent false-success Unit.
            val characteristic = resolveCharacteristic(gatt, charUuid)
                ?: throw GattFailure("characteristic $charUuid not yet discovered")
            gattWriteFailures[gatt]?.let { status ->
                throw GattFailure("previous gatt write failed status=$status")
            }

            // HW-7: Android permits only one outstanding GATT operation per
            // connection (confirmed against bitchat-android's own write
            // serialization). Register the completion future BEFORE issuing
            // the write, so a callback that fires unusually fast can never
            // race ahead of us installing the listener.
            val completion = java.util.concurrent.CompletableFuture<Unit>()
            writeCompletion[gatt] = completion

            // HW-6's bounded retry (kept, narrowed): a synchronous `false`/
            // non-success return from the *initiating* call is Android's
            // documented signal for "another GATT operation already in
            // flight on this connection" — a fast, local, millisecond-scale
            // condition, not "peer unreachable". Retrying the initiation a
            // few times here is still correct; what HW-6 got wrong was
            // stopping at "initiated" instead of waiting for the async
            // onCharacteristicWrite completion below.
            var initiated = false
            for (attempt in 1..GATT_WRITE_RETRY_ATTEMPTS) {
                initiated = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                    gatt.writeCharacteristic(
                        characteristic,
                        data,
                        BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT,
                    ) == BluetoothStatusCodes.SUCCESS
                } else {
                    // API 26-32: 3-arg writeCharacteristic overload is API 33+;
                    // set the write type explicitly then use the legacy 2-arg form.
                    characteristic.value = data
                    characteristic.writeType = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
                    gatt.writeCharacteristic(characteristic)
                }
                if (initiated) break
                if (attempt < GATT_WRITE_RETRY_ATTEMPTS) {
                    Thread.sleep(GATT_WRITE_RETRY_DELAY_MS)
                }
            }
            if (!initiated) {
                writeCompletion.remove(gatt)
                throw GattFailure("gatt write not initiated after $GATT_WRITE_RETRY_ATTEMPTS attempts")
            }

            // Block until onCharacteristicWrite actually fires — this is the
            // HW-7 fix itself. Without this wait, the caller (and the next
            // fragment/MTU-negotiation touching this same connection) could
            // race a write the controller hadn't finished yet.
            try {
                completion.get(GATT_WRITE_TIMEOUT_MS, java.util.concurrent.TimeUnit.MILLISECONDS)
            } catch (e: java.util.concurrent.TimeoutException) {
                writeCompletion.remove(gatt)
                throw GattFailure("gatt write initiated but onCharacteristicWrite never fired within ${GATT_WRITE_TIMEOUT_MS}ms")
            } catch (e: java.util.concurrent.ExecutionException) {
                throw (e.cause as? Exception) ?: GattFailure("gatt write completion failed: ${e.cause}")
            }
        }
    }

    override fun setMtu(handle: ULong, mtu: UShort): UShort {
        return FfiCallTimeout.syncCall(onTimeout = mtu) {
            val gatt = gattHandles[handle.toLong()]
                ?: throw DeviceNotFound()
            gatt.requestMtu(mtu.toInt())
            // AND-RT-111: return the negotiated value (from onMtuChanged) when
            // known, falling back to the requested value.
            negotiatedMtu[gatt]?.toUShort() ?: mtu
        }
    }

    override fun setIdentifyPayload(data: ByteArray) {
        sharedIdentifyPayload = data
    }

    override fun incomingGattWrites(): List<FfiGattWriteEvent> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(pendingGattWrites) }

    override fun scanResults(): List<FfiScanResult> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(pendingScanResults) }

    // HW-9: drained by the Rust accept-poller (ble.rs's ensure_accept_poller)
    // to spawn a reassembly poller for each connection a remote central
    // dialed to us, mirroring incomingGattWrites()/scanResults()'s pattern.
    override fun acceptedConnections(): List<FfiAcceptedConnection> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(pendingAcceptedConnections) }

    override fun drainScanFailures(): List<Int> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(scanFailures) }

    // HV-94: client GATT links that dropped since the last call — the
    // accept-poller tears down the matching `connections` entry + poller.
    override fun drainDisconnectedHandles(): List<Long> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(pendingDisconnectedHandles) }

    // HV-10/HV-31: adapter-lifecycle events (advertising gave up / Bluetooth
    // off / Bluetooth on) — `BleTransport::discover_peers` follows the toggle.
    override fun drainAdapterEvents(): List<Int> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(adapterEvents) }

    /**
     * Atomically removes and returns every buffered item.
     *
     * The previous `toList()` + `clear()` pair was not atomic: a GATT write
     * arriving on the binder thread between the snapshot and the clear was
     * discarded unread. Because the core reassembles multi-frame messages,
     * losing one chunk corrupts a whole message rather than dropping a frame.
     * `poll()` until empty removes exactly what it returns.
     */
    private fun <T> drain(queue: java.util.Queue<T>): List<T> {
        val out = ArrayList<T>(queue.size)
        while (true) out.add(queue.poll() ?: break)
        return out
    }

    // -- platform plumbing -------------------------------------------------

    /**
     * Runs a platform call, translating a revoked runtime permission into the
     * declared FFI error.
     *
     * On Android 12+ every BLE call throws `SecurityException` when its grant is
     * missing, and permissions can be revoked at any time after start — so this
     * is a live path, not a start-up concern.
     */
    private inline fun <T> permitted(block: () -> T): T = try {
        block()
    } catch (e: SecurityException) {
        throw PermissionDenied()
    }

    /**
     * Runs a platform call that has no way to report failure.
     *
     * `stop_scan`, `stop_advertising` and `disconnect_gatt` declare no error
     * type, so a throw becomes UNIFFI_CALL_UNEXPECTED_ERROR on a method with no
     * error channel — an abort. Teardown is best-effort by nature; swallow.
     */
    private inline fun quietly(block: () -> Unit) {
        try {
            block()
        } catch (_: SecurityException) {
        } catch (_: IllegalStateException) {
        } catch (_: NullPointerException) {
        }
    }

    /**
     * Normalises a BLE address to the colon-separated upper-case form the
     * platform requires. Accepts both the bridge's bare-hex form and an already
     * separated address. Returns null when the input is not 12 hex digits.
     */
    private fun toBluetoothMac(address: String): String? {
        val hex = address.filter { it != ':' && it != '-' }.uppercase()
        if (hex.length != 12 || !hex.all { it.isDigit() || it in 'A'..'F' }) return null
        return hex.chunked(2).joinToString(":")
    }

    /**
     * Resolves the write target: prefer the characteristic cached by
     * `onServicesDiscovered` (AND-RT-111), then fall back to a live scan.
     */
    private fun resolveCharacteristic(gatt: BluetoothGatt, charUuidHex: String): BluetoothGattCharacteristic? {
        val cached = serviceCharacteristics[gatt]
        if (cached != null && uuidMatches(cached.uuid, charUuidHex)) return cached
        return findCharacteristic(gatt, charUuidHex)
    }

    private fun uuidMatches(uuid: UUID, charUuidHex: String): Boolean {
        val normalized = hexToUuid(charUuidHex) ?: charUuidHex
        return uuid.toString().equals(normalized, ignoreCase = true)
    }

    private fun findCharacteristic(gatt: BluetoothGatt, charUuidHex: String): BluetoothGattCharacteristic? {
        val uuid = try {
            hexToUuid(charUuidHex)?.let(UUID::fromString)
        } catch (e: Exception) {
            return null
        } ?: return null
        for (service in gatt.services) {
            service.characteristics.firstOrNull { it.uuid == uuid }?.let { return it }
        }
        return null
    }

    private fun buildScanFilters(filter: FfiScanFilter): List<ScanFilter> {
        val builder = ScanFilter.Builder()
        if (filter.serviceUuid.isNotBlank()) {
            try {
                hexToUuid(filter.serviceUuid)?.let { builder.setServiceUuid(ParcelUuid(UUID.fromString(it))) }
            } catch (e: Exception) {
                // Fall through to an unrestricted scan rather than fail the call.
            }
        }
        if (filter.address.isNotBlank()) {
            builder.setDeviceAddress(filter.address)
        }
        return listOf(builder.build())
    }

    private fun scanSettings(): ScanSettings =
        ScanSettings.Builder()
            // HV-91: the app only ever scans while its foreground service is up
            // and it is trying to form or hold a mesh link. SCAN_MODE_LOW_POWER
            // (~512 ms window / ~5 s interval) routinely misses a 1 s-interval
            // advertiser for 10–20 s — the core of "the first message takes
            // ~30 s". LOW_LATENCY scans continuously; the first IRIS beacon is
            // then seen within a second or two. HV-80 will dial this back to
            // BALANCED/LOW_POWER once links are stable and battery matters.
            .setScanMode(ScanSettings.SCAN_MODE_LOW_LATENCY)
            .build()

    private fun pruneOldStarts(now: Long) {
        while (scanStartTimes.isNotEmpty() && now - scanStartTimes.first() > SCAN_WINDOW_MS) {
            scanStartTimes.pollFirst()
        }
    }

    /** `"3e5c6b1a2a104f6e9c315f3e5a0b0c0d"` (32 hex) -> canonical UUID string; null otherwise. */
    private fun hexToUuid(hex: String): String? {
        if (hex.contains('-')) return hex
        if (hex.length != 32 || !hex.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) return null
        return buildString {
            append(hex, 0, 8); append('-')
            append(hex, 8, 12); append('-')
            append(hex, 12, 16); append('-')
            append(hex, 16, 20); append('-')
            append(hex, 20, 32)
        }
    }
}