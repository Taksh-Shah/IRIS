// IOS-001 — Notifications: UNUserNotificationCenter presenter seam.
//
// Standard notifications for delivered messages + a critical-alert seam
// (entitlement-gated; falls back to the standard sound when the app lacks the
// critical-alert entitlement). Emergency P0/P1 delivery never depends on
// notifications — they are a presentation convenience over the D-TN store.
//
// iOS-only (UserNotifications); macOS-host test bundle excludes this file.
import Foundation
import UserNotifications

public protocol NotificationPresenting {
    /// Request authorization (standard options).
    func requestAuthorization() async -> Bool
    /// Post a local notification. `sound` respects the critical-alert
    /// entitlement; falls back to `.default` otherwise.
    func present(title: String, body: String, userInfo: [String: Any], critical: Bool)
}

public final class IrisNotifications: NotificationPresenting {
    private let center: UNUserNotificationCenter

    public init(center: UNUserNotificationCenter = .current()) {
        self.center = center
    }

    public func requestAuthorization() async -> Bool {
        let granted = (try? await center.requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        return granted
    }

    public func present(title: String, body: String, userInfo: [String: Any], critical: Bool) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.userInfo = userInfo
        content.sound = critical
            ? UNNotificationSound.criticalSoundNamed(UNNotificationSoundName.default)
            : UNNotificationSound.default
        let request = UNNotificationRequest(
            identifier: UUID().uuidString,
            content: content,
            trigger: nil
        )
        center.add(request)
    }
}