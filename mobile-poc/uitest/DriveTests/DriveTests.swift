import XCTest

/// Drives the proof-of-concept app with real UIKit touches and on-screen keyboard taps.
/// Coordinates are logical points on a 402-point-wide phone. Screenshots and a keyboard dump go to
/// `POC_OUT`, a host path, because simulator processes share the host file system.
final class DriveTests: XCTestCase {
    private var app: XCUIApplication!
    private let env = ProcessInfo.processInfo.environment
    private var out: String { env["POC_OUT"] ?? "/tmp/mobile-poc-shots/default" }
    private var bundleID: String { env["POC_BUNDLE_ID"] ?? "dev.gpuikit.iospoc" }

    private func shot(_ name: String, settle: TimeInterval = 1.5) {
        Thread.sleep(forTimeInterval: settle)
        let url = URL(fileURLWithPath: "\(out)/\(name).png")
        try? FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)
        try? XCUIScreen.main.screenshot().pngRepresentation.write(to: url)
    }

    private func at(_ x: CGFloat, _ y: CGFloat) -> XCUICoordinate {
        app.coordinate(withNormalizedOffset: .zero).withOffset(CGVector(dx: x, dy: y))
    }

    private func note(_ text: String) {
        let url = URL(fileURLWithPath: "\(out)/notes.txt")
        try? FileManager.default.createDirectory(atPath: out, withIntermediateDirectories: true)
        let line = text + "\n"
        if let handle = try? FileHandle(forWritingTo: url) {
            handle.seekToEndOfFile()
            handle.write(line.data(using: .utf8)!)
            handle.closeFile()
        } else {
            try? line.write(to: url, atomically: true, encoding: .utf8)
        }
    }

    private func launch(scene: String) {
        app = XCUIApplication(bundleIdentifier: bundleID)
        app.launchEnvironment["POC_SCENE"] = scene
        app.launch()
        // Debug builds compile shaders at launch; the launch transition also runs slowly when headless.
        Thread.sleep(forTimeInterval: 12)
    }

    func test1_ScrollAndTap() {
        continueAfterFailure = true
        launch(scene: "top")
        shot("t01-launched-top")

        // A slow drag: the list should follow the finger, with no momentum.
        at(201, 650).press(forDuration: 0.2, thenDragTo: at(201, 250))
        shot("t02-after-slow-drag")

        // Fast flicks: the list should keep moving after the finger lifts.
        for _ in 0..<3 {
            at(201, 700).press(forDuration: 0.01, thenDragTo: at(201, 200), withVelocity: .fast, thenHoldForDuration: 0)
        }
        shot("t03-flick-immediately", settle: 0)
        shot("t04-flick-settled", settle: 3)

        // Kit button in the header.
        at(369, 91).tap()
        shot("t05-tapped-top-button")

        // Kit jump-to-latest button, which shows when scrolled away from the tail.
        at(201, 753).tap()
        shot("t06-tapped-jump-to-latest", settle: 3)

        // Drawer open, then a sidebar row.
        at(28, 91).tap()
        shot("t07-drawer-open")
        at(61, 204).tap()
        shot("t08-sidebar-row-tapped")
    }

    func test2_KeyboardAndComposer() {
        continueAfterFailure = true
        launch(scene: "")
        at(175, 813).tap()
        let keyboard = app.keyboards.firstMatch
        let appeared = keyboard.waitForExistence(timeout: 10)
        note("keyboard appeared after composer tap: \(appeared)")
        shot("k01-composer-tapped")
        guard appeared else { return }
        note("keyboard frame: \(keyboard.frame)")
        note(keyboard.debugDescription)

        for key in ["H", "i"] where keyboard.keys[key].exists {
            keyboard.keys[key].tap()
        }
        shot("k02-typed-hi")

        // The Return key, labelled for the kit's TextInputAction::Send.
        let returnKey = ["send", "Send", "return", "Return"].map { keyboard.buttons[$0] }.first { $0.exists }
        note("return key found: \(returnKey?.label ?? "none")")
        returnKey?.tap()
        shot("k03-after-return-key")

        for key in ["o", "k"] where keyboard.keys[key].exists {
            keyboard.keys[key].tap()
        }
        shot("k04-typed-after-return")

        let backspace = ["delete", "Delete"].map { keyboard.keys[$0] }.first { $0.exists }
        note("delete key found: \(backspace?.label ?? "none")")
        backspace?.tap()
        shot("k05-after-backspace")

        // The kit's send button sits above the keyboard. Path B's keyboard also shows a suggestion
        // bar that XCUITest leaves out of its frame, so the button is about 42 points higher.
        let sendY = keyboard.frame.minY - 26 - (bundleID.hasSuffix("iospocb") ? 39 : 0)
        at(374, sendY).tap()
        shot("k06-send-button", settle: 3)

        // XCUITest's direct typing, which needs an element reporting keyboard focus.
        at(175, sendY).tap()
        Thread.sleep(forTimeInterval: 1)
        note("keyboard focus reported: \(app.descendants(matching: .any).matching(NSPredicate(format: "hasKeyboardFocus == true")).count)")

        // Tap in the transcript, which gpui_ios treats as dismissing the keyboard.
        at(201, 300).tap()
        shot("k07-tapped-transcript", settle: 2)
    }

    func test3_FilterInputAndSelection() {
        continueAfterFailure = true
        launch(scene: "drawer")
        at(149, 86).tap()
        let keyboard = app.keyboards.firstMatch
        let appeared = keyboard.waitForExistence(timeout: 10)
        note("keyboard appeared after filter tap: \(appeared)")
        shot("f01-filter-tapped")
        if appeared {
            for key in ["O", "s"] where keyboard.keys[key].exists { keyboard.keys[key].tap() }
            shot("f02-filter-typed")
            let returnKey = ["done", "Done", "return", "Return"].map { keyboard.buttons[$0] }.first { $0.exists }
            note("filter return key found: \(returnKey?.label ?? "none")")
            returnKey?.tap()
            shot("f03-filter-after-return")
        }
        // Close the drawer through the scrim, then long-press some transcript text.
        at(380, 400).tap()
        shot("f04-scrim-tapped")
        if keyboard.exists { at(380, 200).tap() }
        at(120, 560).press(forDuration: 1.2)
        shot("f05-long-press-selection")
    }
}

extension DriveTests {
    /// Path B only: a native navigation bar and native text field around the GPUI view. Both
    /// sides should keep their own touch and keyboard handling.
    func test4_NativeChrome() throws {
        try XCTSkipUnless(bundleID.hasSuffix("iospocb"), "Needs the Swift host, which only path B has")
        continueAfterFailure = true
        app = XCUIApplication(bundleIdentifier: bundleID)
        app.launchEnvironment["POC_HOST"] = "native"
        app.launchEnvironment["POC_SCENE"] = "top"
        app.launch()
        Thread.sleep(forTimeInterval: 12)
        shot("n01-native-chrome")

        // Scroll the GPUI list inside the native screen.
        at(201, 650).press(forDuration: 0.2, thenDragTo: at(201, 250))
        shot("n02-gpui-scrolled-inside-native")

        // Native text field, native keyboard, native first responder.
        let field = app.textFields["Native UITextField"]
        note("native field exists: \(field.exists)")
        field.tap()
        let keyboard = app.keyboards.firstMatch
        note("keyboard appeared for native field: \(keyboard.waitForExistence(timeout: 10))")
        field.typeText("native")
        shot("n03-native-field-typed")
        note("native field value: \(String(describing: field.value))")

        // A tap on a GPUI button while the native field is first responder. The kit's Top button
        // sits under the native bar's lower edge plus the header row.
        at(369, 91 + (app.navigationBars.firstMatch.frame.maxY - 62)).tap()
        shot("n04-gpui-button-tapped")
    }

    /// Path A only: a native UITextField added on top of the GPUI view through the window handle.
    func test5_NativeOverlay() throws {
        try XCTSkipIf(bundleID.hasSuffix("iospocb"), "Path A experiment")
        continueAfterFailure = true
        app = XCUIApplication(bundleIdentifier: bundleID)
        app.launchEnvironment["POC_HOST"] = "native"
        app.launchEnvironment["POC_SCENE"] = "top"
        app.launch()
        Thread.sleep(forTimeInterval: 12)
        shot("o01-native-overlay")

        // GPUI should still scroll and take taps around the native field.
        at(201, 400).press(forDuration: 0.2, thenDragTo: at(201, 150))
        shot("o02-gpui-scrolled-under-overlay")

        let field = app.textFields["Native UITextField"]
        note("overlay field exists: \(field.exists)")
        field.tap()
        note("keyboard appeared for overlay field: \(app.keyboards.firstMatch.waitForExistence(timeout: 10))")
        field.typeText("native")
        shot("o03-overlay-typed")
        note("overlay field value: \(String(describing: field.value))")

        at(369, 91).tap()
        shot("o04-gpui-button-tapped")
    }
}
