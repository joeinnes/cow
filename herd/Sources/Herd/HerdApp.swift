import SwiftUI

@main
struct HerdApp: App {
    var body: some Scene {
        MenuBarExtra {
            MenuBarView()
                .frame(width: 320)
        } label: {
            Text("🐄")
                .help("Herd — cow pasture manager")
        }
        .menuBarExtraStyle(.window)
    }
}
