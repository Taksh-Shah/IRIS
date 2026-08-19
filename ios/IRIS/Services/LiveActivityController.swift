// IOS-001 AC-13 — LiveActivityController: ActivityKit status Live Activity.
//
// DEC-IOS-0006 / D-8: floor iOS 16.1, availability-gated. The ContentState is
// STATUS-ONLY (neighbor count / relay state / emergency banner) — NEVER message
// payload (screen-locked bystanders must not read message contents). Framing is
// **screen-on only** — scan continuation stops at screen sleep (RES-0024 carry,
// G-IOS-3); this is never a delivery guarantee and never an OS background
// relaxation (supports_background_ios=false unchanged in the Rust matrix).
//
// iOS-only (ActivityKit); macOS-host test bundle excludes this file.
import Foundation
import ActivityKit

@available(iOS 16.1, *)
public struct IrisLiveActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable {
        /// Status-only: e.g. "2 neighbors · relaying".
        public var status: String
    }
    public init() {}
}

@available(iOS 16.1, *)
public final class LiveActivityController {
    public enum AuthState {
        case enabled
        case disabled
        case maxActivities
        case error(String)
    }

    private var activity: Activity<IrisLiveActivityAttributes>?

    public init() {}

    /// ActivityKit authorization posture (areActivitiesEnabled + error surfacing).
    public var authorizationState: AuthState {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            return .disabled
        }
        return .enabled
    }

    /// Start a Live Activity from the FOREGROUND with authorization checked.
    @discardableResult
    public func start(status: String) -> AuthState {
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            return .disabled
        }
        do {
            activity = try Activity.request(
                attributes: IrisLiveActivityAttributes(),
                contentState: IrisLiveActivityAttributes.ContentState(status: status),
                pushType: nil
            )
            return .enabled
        } catch {
            return .error(error.localizedDescription)
        }
    }

    /// Best-effort status refresh (works while the screen is on).
    public func update(status: String) {
        guard let activity else { return }
        Task {
            await activity.update(
                ActivityContent<IrisLiveActivityAttributes.ContentState>(
                    state: IrisLiveActivityAttributes.ContentState(status: status),
                    staleDate: nil
                )
            )
        }
    }

    public func stop() {
        guard let activity else { return }
        Task {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
        self.activity = nil
    }
}