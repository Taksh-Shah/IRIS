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
import iriscode.IrisFfiException
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CopyOnWriteArrayList
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
    }

    private val appContext: Context = context.applicationContext
    private val bleManager: BluetoothManager =
        appContext.getSystemService(Context.BLUETOOTH_SERVICE) as BluetoothManager

    // Handle tables (handle -> live platform object).
    private val nextHandle = AtomicLong(1L)
    private val scanHandles = ConcurrentHashMap<Long, BluetoothLeScanner>()
    private val advertiseHandles = ConcurrentHashMap<Long, BluetoothLeAdvertiser>()
    private val gattHandles = ConcurrentHashMap<Long, BluetoothGatt>()

    // Drain buffers (proj-BLE-1 — owned projection of the core drains).
    private val pendingScanResults = CopyOnWriteArrayList<FfiScanResult>()
    private val pendingGattWrites = CopyOnWriteArrayList<FfiGattWriteEvent>()

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
    private var gattServer: BluetoothGattServer? = null

    private val gattServerCallback = object : BluetoothGattServerCallback() {
        override fun onCharacteristicWriteRequest(
            device: BluetoothDevice,
            requestId: Int,
            characteristic: BluetoothGattCharacteristic,
            preparedWrite: Boolean,
            responseNeeded: Boolean,
            value: ByteArray,
            offset: Int,
        ) {
            if (characteristic.uuid == IRIS_CHARACTERISTIC_UUID) {
                pendingGattWrites.add(
                    FfiGattWriteEvent(
                        handle = deviceHash(device),
                        charUuid = characteristic.uuid.toString(),
                        data = value,
                    ),
                )
            }
            gattServer?.sendResponse(device, requestId, BluetoothGatt.GATT_SUCCESS, offset, null)
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
        val server = bleManager.openGattServer(appContext, gattServerCallback) ?: return
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
        rssiFloor = filter.rssiFloor
        return FfiCallTimeout.syncCall(onTimeout = 0uL) {
            val scanner = bleManager.adapter.bluetoothLeScanner ?: return@syncCall 0uL
            val now = SystemClock.elapsedRealtime()
            pruneOldStarts(now)
            if (scanStartTimes.size >= SCAN_RESTART_CEILING) return@syncCall 0uL
            scanStartTimes.add(now)

            val filters = buildScanFilters(filter)
            scanner.startScan(filters, scanSettings(), scanCallback)
            val handle = nextHandle.getAndIncrement()
            scanHandles[handle] = scanner
            handle.toULong()
        }
    }

    override fun stopScan(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            scanHandles.remove(handle.toLong())?.stopScan(scanCallback)
        }
    }

    override fun startAdvertising(data: FfiAdvertisementData): ULong {
        return FfiCallTimeout.syncCall(onTimeout = 0uL) {
            val advertiser = bleManager.adapter.bluetoothLeAdvertiser ?: return@syncCall 0uL
            ensureGattServer()
            val settings = AdvertiseSettings.Builder()
                .setAdvertiseMode(AdvertiseSettings.ADVERTISE_MODE_LOW_POWER)
                .setTxPowerLevel(AdvertiseSettings.ADVERTISE_TX_POWER_MEDIUM)
                .setConnectable(!data.nonConnectable)
                .build()
            val adData = AdvertiseData.Builder()
                .setServiceData(ParcelUuid(IRIS_SERVICE_UUID), data.payload)
                .build()
            if (adData.serviceData.isEmpty()) return@syncCall 0uL
            advertiser.startAdvertising(settings, adData, advertiseCallback)
            val handle = nextHandle.getAndIncrement()
            advertiseHandles[handle] = advertiser
            handle.toULong()
        }
    }

    override fun stopAdvertising(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            advertiseHandles.remove(handle.toLong())?.stopAdvertising(advertiseCallback)
        }
    }

    override fun connectGatt(address: String): ULong {
        return FfiCallTimeout.syncCall(onTimeout = 0uL) {
            val device: BluetoothDevice = bleManager.adapter.getRemoteDevice(address)
            val gatt = device.connectGatt(appContext, autoConnect = false, callback = gattCallback)
                ?: return@syncCall 0uL
            val handle = nextHandle.getAndIncrement()
            gattHandles[handle] = gatt
            handle.toULong()
        }
    }

    override fun disconnectGatt(handle: ULong) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            gattHandles.remove(handle.toLong())?.let { gatt ->
                gatt.disconnect()
                gatt.close()
            }
        }
    }

    override fun gattWrite(handle: ULong, charUuid: String, data: ByteArray) {
        FfiCallTimeout.syncCall(onTimeout = Unit) {
            val gatt = gattHandles[handle.toLong()]
                ?: throw IrisFfiException.GattFailure("unknown gatt connection")
            // AND-RT-111: a write before discovery resolves to a typed error,
            // never a silent false-success Unit.
            val characteristic = resolveCharacteristic(gatt, charUuid)
                ?: throw IrisFfiException.GattFailure("characteristic $charUuid not yet discovered")
            gattWriteFailures[gatt]?.let { status ->
                throw IrisFfiException.GattFailure("previous gatt write failed status=$status")
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
                    throw IrisFfiException.GattFailure("gatt write not initiated")
                }
            }
        }
    }

    override fun setMtu(handle: ULong, mtu: UShort): UShort {
        return FfiCallTimeout.syncCall(onTimeout = mtu) {
            val gatt = gattHandles[handle.toLong()]
                ?: throw IrisFfiException.DeviceNotFound()
            gatt.requestMtu(mtu.toInt())
            // AND-RT-111: return the negotiated value (from onMtuChanged) when
            // known, falling back to the requested value.
            negotiatedMtu[gatt]?.toUShort() ?: mtu
        }
    }

    override fun incomingGattWrites(): List<FfiGattWriteEvent> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) {
            val drained = pendingGattWrites.toList()
            pendingGattWrites.clear()
            drained
        }

    override fun scanResults(): List<FfiScanResult> =
        FfiCallTimeout.syncCall(onTimeout = emptyList()) {
            val drained = pendingScanResults.toList()
            pendingScanResults.clear()
            drained
        }

    // -- platform plumbing -------------------------------------------------

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