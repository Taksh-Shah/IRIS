package iriscore.adapter

import java.io.File
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * HV-16: the IRIS BLE service / characteristic UUIDs are mirrored in three
 * places — `iris_core::transport::ble` (the wire truth), this Android adapter,
 * and iOS `IrisBleConstants`. They drift silently, and when they do every real
 * BLE write fails ("characteristic not yet discovered", BLE-3) or the scanner
 * never matches the peripheral (CROSS-001). Nothing was guarding it.
 *
 * This test parses the canonical hex strings out of the Rust regression test
 * (`crates/iris-android/src/bridge.rs::hv16_ble_uuids_match_the_canonical_wire_values`,
 * which in turn asserts them against the core `pub const`s) and fails the
 * Android build if this adapter's constants no longer agree. Changing a UUID
 * now means deliberately editing BOTH tests (and iOS).
 */
class BleUuidParityTest {

    private val repoRoot: File by lazy {
        // JVM test working directory is android/app.
        var d: File = File("").absoluteFile
        while (!File(d, "Cargo.toml").exists()) {
            d = d.parentFile ?: error("could not find repo root (no Cargo.toml above ${File("").absoluteFile})")
        }
        d
    }

    /** hex string with dashes/colons stripped, lower-cased. */
    private fun norm(s: String) = s.replace("-", "").replace(":", "").lowercase()

    /** The three `"<32 hex>"` literals asserted in the Rust HV-16 test, in order. */
    private val canonical: List<String> by lazy {
        val src = File(repoRoot, "crates/iris-android/src/bridge.rs").readText()
        val body = src.substringAfter("fn hv16_ble_uuids_match_the_canonical_wire_values")
            .substringBefore("assert_ne!")
        Regex("\"([0-9a-fA-F]{32})\"").findAll(body).map { it.groupValues[1].lowercase() }.toList()
    }

    @Test
    fun service_uuid_matches_the_rust_wire_value() {
        assertTrue(canonical.size >= 3, "could not parse the 3 canonical UUIDs from bridge.rs; got $canonical")
        assertEquals(
            canonical[0],
            norm(AndroidBleTransportAdapter.IRIS_SERVICE_UUID.toString()),
            "IRIS_SERVICE_UUID drifted from iris_core — the scanner filter and the " +
                "advertised service will no longer match a Rust/iOS peer",
        )
    }

    @Test
    fun write_characteristic_matches_the_rust_wire_value() {
        assertEquals(
            canonical[2],
            norm(AndroidBleTransportAdapter.IRIS_CHARACTERISTIC_UUID.toString()),
            "IRIS_CHARACTERISTIC_UUID drifted from iris_core::IRIS_WRITE_CHARACTERISTIC — " +
                "every inbound frame will miss the GATT-server characteristic match",
        )
    }

    @Test
    fun the_service_and_characteristic_uuids_do_not_collide() {
        assertNotEquals(
            AndroidBleTransportAdapter.IRIS_SERVICE_UUID,
            AndroidBleTransportAdapter.IRIS_CHARACTERISTIC_UUID,
            "writing to the service UUID instead of the characteristic was exactly the BLE-3 bug",
        )
    }
}
