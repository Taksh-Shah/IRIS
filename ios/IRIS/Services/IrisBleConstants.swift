// IOS-001 AC-4..AC-9 — IRIS GATT domain constants (DEC-BLE-002-0001/0002).
//
// The stable IRIS service UUID and identify characteristic MUST match
// `crates/iris-core/src/transport/ble.rs`:
//   IRIS_SERVICE_UUID            = Uuid([1,0,...]) = 01000000-0000-0000-0000-000000000000
//   IRIS_IDENTIFY_CHARACTERISTIC = Uuid([2,0,...]) = 02000000-0000-0000-0000-000000000000
// The FFI passes 32-hex strings (crate::bridge uuid_to_hex); CoreBluetooth
// needs canonical dashed CBUUID forms. Restoration identifiers are the
// DEC-BLE-002-0005 constants ("IrisCentralManager"/"IrisPeripheralManager").
import Foundation
import CoreBluetooth

public enum IrisBleConstants {
    /// DEC-BLE-002-0001 stable IRIS GATT service UUID (hex, core representation).
    public static let serviceUUIDHex = "01000000000000000000000000000000"
    /// DEC-BLE-002-0002 connect-to-identify characteristic UUID (hex).
    public static let identifyCharacteristicHex = "02000000000000000000000000000000"
    /// Control/write characteristic the core writes frames to (BLE-002 §5).
    /// Must be distinct from serviceUUID — collision breaks GATT routing.
    public static let controlCharacteristicHex = "03000000000000000000000000000000"

    public static let serviceUUID: CBUUID = CBUUID(string: "01000000-0000-0000-0000-000000000000")
    public static let identifyUUID: CBUUID = CBUUID(string: "02000000-0000-0000-0000-000000000000")
    public static let controlUUID: CBUUID = CBUUID(string: "03000000-0000-0000-0000-000000000000")

    /// DEC-BLE-002-0005 restoration identifiers (stable, never per-session).
    public static let centralRestoreIdentifier = "IrisCentralManager"
    public static let peripheralRestoreIdentifier = "IrisPeripheralManager"

    /// Advertised local name — ≤ 10 bytes (§4: LocalNameKey ≤ 10 B).
    public static let localName = "IRIS"
    public static let localNameMaxBytes = 10

    /// 12-hex token for an identity UUID: the first 6 bytes of the identifier
    /// (48 bits). Round-trips through `crate::bridge`'s 6-byte BleAddress hex
    /// conversion (`ble_addr_to_hex` emits 12 uppercase hex chars); collision
    /// risk over 2^48 with a ~8-peer bound (§4) is negligible.
    public static func token(for identifier: UUID) -> String {
        let hex = identifier.uuidString.replacingOccurrences(of: "-", with: "")
        return String(hex.prefix(12)).uppercased()
    }

    /// 32-hex (dash/colon tolerant) → canonical CBUUID string; nil if invalid.
    public static func canonicalUUIDString(_ hex: String) -> String? {
        var h = hex
        h.removeAll { $0 == "-" || $0 == ":" }
        guard h.count == 32, h.allSatisfy({ $0.isHexDigit }) else { return nil }
        let c = Array(h)
        return String([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])
            + "-" + String([c[8], c[9], c[10], c[11]])
            + "-" + String([c[12], c[13], c[14], c[15]])
            + "-" + String([c[16], c[17], c[18], c[19]])
            + "-" + String(c[20...31])
    }

    public static func cbuuid(hex32: String) -> CBUUID? {
        canonicalUUIDString(hex32).map { CBUUID(string: $0) }
    }

    /// CBUUID → 32-hex (no dashes); the bridge's UUID representation.
    public static func toHex32(_ uuid: CBUUID) -> String {
        let dashed = uuid.uuidString
        return dashed.replacingOccurrences(of: "-", with: "")
    }
}