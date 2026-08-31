// IOS-001 — Notifications: UNUserNotificationCenter presenter seam.
//
// Standard notifications for delivered messages + a critical-alert seam
// (entitlement-gated; without the entitlement the OS plays no sound — there is
// no runtime fallback swap). Emergency P0/P1 delivery never depends on
// notifications — they are a presentation convenience over the D-TN store.
//
// iOS-only (UserNotifications); macOS-host test bundle excludes this file.
import Foundation
import UserNotifications
import os.log

public protocol NotificationPresenting {
    /// Request authorization (standard options).
    func requestAuthorization() async -> Bool
    /// Post a local notification. `sound` requests the critical-alert
    /// entitlement ONLY when `critical`; without the entitlement the OS
    /// delivers the notification silently (no fallback sound swap exists at
    /// runtime), so P0/P1 delivery NEVER depends on notifications.
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
        // Bug #37: criticalSoundNamed(.default) is invalid; use defaultCritical.
        content.sound = critical ? .defaultCritical : .default
        let request = UNNotificationRequest(
            identifier: UUID().uuidString,
            content: content,
            trigger: nil
        )
        // Bug #37: add a completion handler so scheduling errors are visible.
        center.add(request) { error in
            if let error {
                os_log("ios: notification schedule failed: %@", type: .error, error.localizedDescription)
            }
        }
    }
}