// IOS-001 AC-13 — IRIS Live Activity widget extension (ActivityKit).
//
// Minimal ContentState (status only, no payload). The widget renders the same
// `IrisLiveActivityAttributes` the app pushes; decode must never leak message
// content (status strings are renderer-produced summaries only).
import WidgetKit
import SwiftUI
import ActivityKit

@main
struct IrisLiveActivityWidgetBundle: WidgetBundle {
    var body: some Widget {
        IrisLiveActivityWidget()
    }
}

struct IrisLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: IrisLiveActivityAttributes.self) { context in
            LiveActivityView(
                status: context.state.status,
                isStale: false
            )
            .activityBackgroundTint(Color.black.opacity(0.9))
            .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Text("IRIS").font(.headline).foregroundColor(.white)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.state.status).font(.footnote)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    Text("Mesh relay active").font(.caption2).foregroundColor(.secondary)
                }
            } compactLeading: {
                Text("IRIS")
            } compactTrailing: {
                Image(systemName: "dot.radiowaves.left.and.right")
            } minimal: {
                Image(systemName: "dot.radiowaves.left.and.right")
            }
        }
    }
}

private struct LiveActivityView: View {
    let status: String
    let isStale: Bool

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 4) {
                Text("IRIS Mesh").font(.headline)
                Text(status)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            Spacer()
            Image(systemName: "dot.radiowaves.left.and.right")
                .foregroundColor(isStale ? .secondary : .green)
        }
        .padding()
    }
}