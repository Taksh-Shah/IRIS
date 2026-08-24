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
    private val advertiseHandles = ConcurrentHashMap<Long, BluetoothLeAdvertiser>()
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
            val bytes = result.scanRecord?.bytes ?: ByteArray(0)
            while (pendingScanResults.size >= MAX_PENDING) pendingScanResults.poll()
            pendingScanResults.add(
                FfiScanResult(
                    address = device.address,
                    payload = bytes,
                    rssi = result.rssi,
                ),
            )
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
            if (status != BluetoothGatt.GATT_SUCCESS) return
            gatt.getService(IRIS_SERVICE_UUID)?.getCharacteristic(IRIS_CHARACTERISTIC_UUID)?.let {
                serviceCharacteristics[gatt] = it
            }
        }

        override fun onConnectionStateChange(gatt: BluetoothGatt, status: Int, newState: Int) {
            if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                gattHandles.entries.removeIf { it.value == gatt }
                serviceCharacteristics.remove(gatt)
                negotiatedMtu.remove(gatt)
                gattWriteFailures.remove(gatt)
                gatt.close()
            }
        }
    }

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

    private val advertiseCallback = object : AdvertiseCallback() {
        override fun onStartSuccess(settingsInEffect: AdvertiseSettings) = Unit
        override fun onStartFailure(errorCode: Int) = Unit
    }

    override fun startScan(filter: FfiScanFilter): ULong {
        return FfiCallTimeout.syncCall(onTimeout = throwTimeout()) {
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
        return FfiCallTimeout.syncCall(onTimeout = throwTimeout()) {
            val advertiser = bleManager?.adapter?.bluetoothLeAdvertiser
                ?: throw AdapterOff()
            ensureGattServer()
            val settings = AdvertiseSettings.Builder()
                .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_POWER)
                .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM)
                .setConnectable(!data.nonConnectable)
                .build()
            val adData = AdvertiseData.Builder()
                .addServiceData(ParcelUuid(IRIS_SERVICE_UUID), data.payload)
                .build()
            if (adData.serviceData.isEmpty()) {
                throw InvalidArgument("advertisement payload produced no service data")
            }
            permitted { advertiser.startAdvertising(settings, adData, advertiseCallback) }
            val handle = nextHandle.getAndIncrement()
            advertiseHandles[handle] = advertiser
            handle.toULong()
        }
    }

    override fun stopAdvertising(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            quietly { advertiseHandles.remove(handle.toLong())?.stopAdvertising(advertiseCallback) }
        }
    }

    override fun connectGatt(address: String): ULong {
        return FfiCallTimeout.syncCall(onTimeout = 0uL) {
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
                if (!gatt.writeCharacteristic(characteristic)) {
                    // 2-arg form returns Boolean; a false return means the
                    // platform refused the write — surface as a typed error.
                    throw GattFailure("gatt write not initiated")
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
     * `syncCall` needs an `onTimeout` value of the op's return type. These ops
     * have no benign default — a fabricated handle is worse than an error — so
     * the fallback throws instead.
     */
    private fun throwTimeout(): ULong = throw FfiTimeout()

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