// IOS-001 — IRISApp: SwiftUI entry point.
import SwiftUI

@main
struct IRISApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}

/// Minimal shell UI: node short-id + mesh status. The engine/Rust core carries
/// all protocol logic; this surface is deliberately thin (IOS_DESIGN §3).
struct ContentView: View {
    @State private var statusText = "IRIS · starting"

    var body: some View {
        VStack(spacing: 16) {
            Text("IRIS Mesh")
                .font(.title)
            Text(statusText)
                .font(.caption)
                .monospaced()
            Button("Start mesh") {
                do {
                    try AppDelegate.shared.engine?.startAll()
                    statusText = "IRIS · mesh up"
                } catch {
                    statusText = "IRIS · error: \(error.localizedDescription)"
                }
            }
            .buttonStyle(.borderedProminent)
        }
        .padding()
        .task {
            if let engine = AppDelegate.shared.engine {
                let id = engine.nodeId().prefix(8).map { String(format: "%02x", $0) }.joined()
                statusText = "IRIS · node \(id)"
            }
        }
    }
}