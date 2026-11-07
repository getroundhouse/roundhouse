import SwiftUI

// Xcode requires a host for a UI test bundle. The tests drive another app by bundle identifier.
@main
struct HostApp: App {
    var body: some Scene { WindowGroup { Text("Test host") } }
}
