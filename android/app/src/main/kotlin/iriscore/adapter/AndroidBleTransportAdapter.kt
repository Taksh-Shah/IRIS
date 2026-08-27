package iriscore.adapter

import android.bluetooth.BluetoothDevice
import android.bluetooth.BluetoothGatt
import android.bluetooth.BluetoothGattCallback
import android.bluetooth.BluetoothGattCharacteristic
import android.bluetooth.BluetoothGattServer
import android.bluetooth.BluetoothGattServerCallback
import android.bluetooth.BluetoothGattService
import android.bluetooth.BluetoothManager
import android.content.Context
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
import iriscode.FfiAdvertisementData
import iriscode.FfiBleAdapter
import iriscode.FfiGattWriteEvent
import iriscode.FfiScanFilter
import iriscode.FfiScanResult
import iriscode.AdapterOff
import iriscode.DeviceNotFound
import iriscode.FfiTimeout
import iriscode.GattFailure
import iriscode.InvalidArgument
import iriscode.PermissionDenied
import iriscode.TransportFailure
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.ConcurrentSkipListSet
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
        /** IRIS BLE transport service UUID (BLE-001). */
        val IRIS_SERVICE_UUID: UUID = UUID.fromString("3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0d")

        /** GATT write characteristic used by the core BLE-001 transport. */
        val IRIS_CHARACTERISTIC_UUID: UUID = UUID.fromString("3e5c6b1a-2a10-4f6e-9c31-5f3e5a0b0c0e")

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
    private val pendingGattWrites = ConcurrentLinkedQueue<FfiGattWriteEvent>()

    // AND-RT-111: IRIS characteristic cached by onServicesDiscovered (async);
    // negotiated MTU + last client-write status tracked per connection.
    private val serviceCharacteristics = ConcurrentHashMap<BluetoothGatt, BluetoothGattCharacteristic>()
    private val negotiatedMtu = ConcurrentHashMap<BluetoothGatt, Int>()
    private val gattWriteFailures = ConcurrentHashMap<BluetoothGatt, Int>()

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
            // Was unoverridden — the platform's own restart-throttle refusal
            // (SCAN_FAILED_ALREADY_STARTED et al.) was invisible; the core's
            // own scan_allowed() backoff had no way to see it either.
            android.util.Log.w("IrisBle", "startScan failed errorCode=$errorCode")
        }
    }

    /** RSSI floor applied on the platform side (core filter default -95 dBm). */
    @Volatile
    private var rssiFloor: Int = -127

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
            gatt.getService(IRIS_SERVICE_UUID)?.getCharacteristic(IRIS_CHARACTERISTIC_UUID)?.let {
                serviceCharacteristics[gatt] = it
            }
            // Connection is genuinely usable now: STATE_CONNECTED happened AND
            // the characteristic gattWrite needs is resolvable. This is what
            // connectGatt() has been blocking on.
            connectionReady.remove(gatt)?.complete(Unit)
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
                gattHandles.entries.removeIf { it.value == gatt }
                serviceCharacteristics.remove(gatt)
                negotiatedMtu.remove(gatt)
                gattWriteFailures.remove(gatt)
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
    @Volatile
    private var gattServer: BluetoothGattServer? = null

    private val gattServerCallback = object : BluetoothGattServerCallback() {
        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            offset: Int,
            value: ByteArray,
        ) {
            if (characteristic.uuid == IRIS_CHARACTERISTIC_UUID) {
                // Bounded: if the core stops draining, drop the oldest rather
                // than growing without limit.
                while (pendingGattWrites.size >= MAX_PENDING) pendingGattWrites.poll()
                pendingGattWrites.add(
                    FfiGattWriteEvent(
                        handle = deviceHash(device),
                        charUuid = characteristic.uuid.toString(),
                        data = value,
                    ),
                )
            }
            // Only the platform contract's "response needed" writes get a reply;
            // an unsolicited sendResponse on a WRITE_NO_RESPONSE write is a
            // protocol violation on the GATT server side.
            if (responseNeeded) {
                // Binder thread: nothing above can catch a throw here.
                quietly {
                    gattServer?.sendResponse(
                        device,
                        requestId,
                        BluetoothGatt.GATT_SUCCESS,
                        offset,
                        null,
                    )
                }
            }
        }
    }

    /**
     * AND-RT-113: collision-resistant per-device key used as the FFI `handle`
     * projection for server writes. A 64-bit SHA-256 digest over the MAC bytes
     * replaces the 31-bit `hashCode()` (whose collisions misattribute frames
     * across peers).
     */
    private fun deviceHash(device: BluetoothDevice): ULong {
        val address = device.address.replace(":", "").lowercase()
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(address.toByteArray(Charsets.US_ASCII))
        return (ByteBuffer.wrap(digest).long and Long.MAX_VALUE).toULong()
    }

    /** Open the process-scoped GATT server once and serve the IRIS transport characteristic. */
    private fun ensureGattServer() {
        if (gattServer != null) return
        val server = bleManager?.openGattServer(appContext, gattServerCallback) ?: return
        val service = BluetoothGattService(
            IRIS_SERVICE_UUID,
            BluetoothGattService.SERVICE_TYPE_PRIMARY,
        )
        service.addCharacteristic(
            BluetoothGattCharacteristic(
                IRIS_CHARACTERISTIC_UUID,
                BluetoothGattCharacteristic.PROPERTY_WRITE or
                    BluetoothGattCharacteristic.PROPERTY_WRITE_NO_RESPONSE or
                    BluetoothGattCharacteristic.PROPERTY_NOTIFY,
                BluetoothGattCharacteristic.PERMISSION_WRITE,
            ),
        )
        if (server.addService(service)) gattServer = server
    }

    private val advertiseHandles = ConcurrentHashMap<Long, Pair<BluetoothLeAdvertiser, AdvertiseCallback>>()

    override fun startScan(filter: FfiScanFilter): ULong {
        // Was `syncCall(onTimeout = throwTimeout())`: throwTimeout() throws
        // eagerly while Kotlin builds syncCall's argument list, before the
        // block below - the real scan start - ever runs. Every call to
        // startScan threw FfiTimeout unconditionally. syncCallOrThrow defers
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
                throw TransportFailure("BLE scan restart ceiling reached")
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
        // threw FfiTimeout unconditionally, before advertising was ever
        // attempted. Confirmed on a real device (API 34): every call failed
        // immediately with "transport protocol error: timeout", under 2s,
        // nowhere near the 30s budget - because the real operation never ran.
        return FfiCallTimeout.syncCallOrThrow {
            val advertiser = bleManager?.adapter?.bluetoothLeAdvertiser
                ?: throw AdapterOff()
            ensureGattServer()
            val settings = AdvertiseSettings.Builder()
                .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_POWER)
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
            val callback = object : AdvertiseCallback() {
                override fun onStartFailure(errorCode: Int) {
                    // Was `= Unit` — every legacy-budget overflow (errorCode 1,
                    // ADVERTISE_FAILED_DATA_TOO_LARGE) failed completely
                    // silently and looked identical to a successful start.
                    android.util.Log.w("IrisBle", "startAdvertising failed errorCode=$errorCode")
                    advertiseHandles.remove(handle)
                }
            }
            advertiseHandles[handle] = advertiser to callback
            permitted { advertiser.startAdvertising(settings, adData, callback) }
            handle.toULong()
        }
    }

    override fun stopAdvertising(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            quietly {
                advertiseHandles.remove(handle.toLong())?.let { (advertiser, callback) ->
                    advertiser.stopAdvertising(callback)
                }
            }
        }
    }

    override fun connectGatt(address: String): ULong {
        // Was `syncCall(onTimeout = 0uL)`: on a real overrun (device out of
        // range, ACL handshake stuck, ...) that silently returned handle 0
        // as if it were a genuine connection — Rust had no way to tell a
        // timed-out connect from a successful one, so it proceeded straight
        // to gattWrite() against a handle nothing was ever registered
        // under. syncCallOrThrow surfaces the overrun as FfiTimeout instead.
        return FfiCallTimeout.syncCallOrThrow {
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
            val gatt = permitted { device.connectGatt(appContext, false, gattCallback) }
                ?: throw DeviceNotFound()
            android.util.Log.d("IrisBleDiag", "connectGatt: initiated, waiting for ready")
            val ready = java.util.concurrent.CompletableFuture<Unit>()
            connectionReady[gatt] = ready
            // BLE-2: block this watchdog-pool thread (never the main/Binder
            // thread — see FfiCallTimeout) until gattCallback resolves
            // [ready]: STATE_CONNECTED *and* service discovery both
            // completed, or a failure. The outer syncCall's own timeout
            // (30s) is the backstop if neither ever arrives.
            try {
                ready.get()
                android.util.Log.d("IrisBleDiag", "connectGatt: ready resolved successfully")
            } catch (e: java.util.concurrent.ExecutionException) {
                android.util.Log.w("IrisBleDiag", "connectGatt: ready failed", e)
                connectionReady.remove(gatt)
                throw (e.cause as? Exception) ?: GattFailure("connect failed: ${e.cause}")
            }
            val handle = nextHandle.getAndIncrement()
            gattHandles[handle] = gatt
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
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            android.util.Log.d("IrisBleDiag", "gattWrite ENTER handle=$handle len=${data.size} known=${gattHandles.keys}")
            val gatt = gattHandles[handle.toLong()]
                ?: throw GattFailure("unknown gatt connection")
            // AND-RT-111: a write before discovery resolves to a typed error,
            // never a silent false-success Unit.
            val characteristic = resolveCharacteristic(gatt, charUuid)
                ?: throw GattFailure("characteristic $charUuid not yet discovered")
            gattWriteFailures[gatt]?.let { status ->
                throw GattFailure("previous gatt write failed status=$status")
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                gatt.writeCharacteristic(
                    characteristic,
                    data,
                    BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT,
                )
            } else {
                // API 26-32: 3-arg writeCharacteristic overload is API 33+;
                // set the write type explicitly then use the legacy 2-arg form.
                characteristic.writeType = BluetoothGattCharacteristic.WRITE_TYPE_DEFAULT
                // HW-6: a `false` return here is Android's documented signal
                // for "another GATT operation is already in flight on this
                // connection" — NOT a dead link. Confirmed live and
                // reproducible on a real API <33 device: "gatt write not
                // initiated" on the very first send attempt after a
                // connection the engine had already confirmed live and
                // idle. Surfacing this as a hard GattFailure sent it all
                // the way up to the message engine's OWN retry backoff
                // (600s initial timeout for P4/bulk, HW-4) — wildly
                // mismatched with the millisecond-scale transient
                // contention this actually is. A short bounded retry here,
                // at the layer that actually knows this is a fast local
                // condition (not "peer unreachable"), is the standard fix
                // for this exact Android BLE API behavior.
                var initiated = false
                for (attempt in 1..GATT_WRITE_RETRY_ATTEMPTS) {
                    if (gatt.writeCharacteristic(characteristic)) {
                        initiated = true
                        break
                    }
                    if (attempt < GATT_WRITE_RETRY_ATTEMPTS) {
                        Thread.sleep(GATT_WRITE_RETRY_DELAY_MS)
                    }
                }
                if (!initiated) {
                    throw GattFailure("gatt write not initiated after $GATT_WRITE_RETRY_ATTEMPTS attempts")
                }
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

    override fun incomingGattWrites(): List<FfiGattWriteEvent> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(pendingGattWrites) }

    override fun scanResults(): List<FfiScanResult> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) { drain(pendingScanResults) }

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
            .setScanMode(ScanSettings.SCAN_MODE_LOW_POWER)
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