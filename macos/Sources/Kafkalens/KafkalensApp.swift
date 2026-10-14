import AppKit
import SwiftUI

@main
struct KafkalensApp: App {
    @ViewState private var profiles = ProfileStore()

    init() {
        // Needed when launched as a bare executable (`swift run`) rather than
        // from the .app bundle, otherwise the window never takes focus.
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.activate()
    }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(profiles)
        }
        .defaultSize(width: 1280, height: 800)
        .commands {
            SidebarCommands()
            InspectorCommands()
        }
    }
}
