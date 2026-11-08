# Mobile proof of concept

How to build and run the mobile proof of concept: one gpui-kit screen, `shared/demo.rs`, on two iOS backends. It's a standalone prototype, outside the Cargo workspace.

| | Path A | Path B |
|---|---|---|
| Backend | Zed's `gpui-pre-ios` 0.3.11 | Longbridge's `gpui-pre-mobile` at `7548ebc`, bumped to GPUI 0.3.11 |
| App | Rust only, plus an Xcode stub for signing | A Rust static library and a Swift host |
| Bundle ID | `dev.gpuikit.iospoc` | `dev.gpuikit.iospocb` |

```text
shared/demo.rs      The screen, used by both paths: 5,000 generated rows of mixed height
path-a/             gpui_ios app
path-b/             gpui-pre-mobile app
patches/            The diffs we maintain. patches/build/ is generated and ignored
scripts/            prepare-patches.sh, device.sh
uitest/             XCUITest driver for touches and keyboard taps
test-server/        A simulated hub, with its own README
```

## In the simulator

Run these from `mobile-poc/`.

```sh
# Path A
path-a/scripts/build.sh sim          # or: device [--release]
xcrun simctl boot <device-id>
xcrun simctl install <device-id> path-a/build/sim-debug/GpuiKitPoc.app
SIMCTL_CHILD_POC_SCENE=top xcrun simctl launch <device-id> dev.gpuikit.iospoc

# Path B (the bundle path is printed by the build)
path-b/scripts/build.sh sim          # or: device [--release]
xcrun simctl install <device-id> path-b/build/DerivedData/Build/Products/Debug-iphonesimulator/GpuiKitPocB.app
SIMCTL_CHILD_POC_SCENE=top xcrun simctl launch <device-id> dev.gpuikit.iospocb

# Touch and keyboard driver, against an installed app on a booted simulator
uitest/run.sh <device-id> dev.gpuikit.iospoc /tmp/shots-a
```

- **Scenes (`POC_SCENE`):** `top`, `mid`, `drawer`, `focus`, or empty for the tail.
- **Native UI (`POC_HOST=native`):** on path B this adds a native navigation bar and text field around GPUI. On path A it adds a native text field on top of GPUI.
- **Event log:** the app writes `Documents/poc.log` in its data container, because a screenshot can't show whether an event arrived. Its `keystroke:` lines show which keys arrive.
- **Wait about 14 seconds before a screenshot.** Debug builds show a white launch screen for 2 to 3 seconds, and shots taken within about 5 seconds of launch show a gray veil, probably the fade from that white screen.

`simctl` can't inject touches, so `uitest/` uses XCUITest to send real UIKit touches and on-screen keyboard taps to a headless simulator. GPUI elements are invisible to XCUITest, so the driver taps by coordinates. XCUITest can't send hardware keys, because `typeKey` isn't in the iOS SDK headers, so those need a person.

## On an iPhone

The phone needs Developer Mode on, under Settings > Privacy & Security > Developer Mode, followed by a restart. Signing needs a team with an Apple Development certificate, since a Developer ID certificate can't sign for an iPhone. `security find-identity -v -p codesigning` lists the certificates, and `xcrun devicectl list devices` lists the devices.

Connect the phone by cable, unlock it, and accept any trust prompt. Then, from `mobile-poc/`:

```sh
TEAM_ID=<team> DEVICE=<device-id> scripts/device.sh a
TEAM_ID=<team> DEVICE=<device-id> scripts/device.sh b
```

This lets Xcode manage signing, so it registers the device and bundle ID and creates a development profile in that team. If a bundle ID is already taken, add `BUNDLE_ID_SUFFIX=.yourname`. If iOS blocks the first launch, trust the developer under Settings > General > VPN & Device Management.

## Patches and workarounds

### Both paths

| Item | Why | Maintenance |
|---|---|---|
| Explicit monospace font (`probe_mono_font` in `shared/demo.rs`) | The kit's default names a Linux font on iOS and silently falls back to the proportional system font. GPUI panics on a font it can't load, so each candidate is loaded through the platform first | Low. App code |
| `.occlude()` on the drawer panel | Without it, a tap on a row also hits the scrim behind it. The bug is the demo's, not either backend's | None |

### Path A

| Item | Why | Maintenance |
|---|---|---|
| `patches/gpui-base-0.9.0-text-length.patch`: one method, `text_length_utf16`, on the kit's input handler | **Required.** Without it the keyboard never appears. `gpui_ios` won't show it or apply edits unless it knows the document's length, and the kit never reports one | Low. 10 lines, re-applied on each kit release until gpui-kit takes it |
| `runtime_shaders` feature on `gpui-pre-ios` | It avoids needing Xcode's Metal Toolchain component. The kit uses the same feature on desktop | None. `xcodebuild -downloadComponent MetalToolchain` would allow precompiled shaders |
| `ios/project.yml` with a stub target | Only for signing: Xcode signs a stub whose executable is replaced by the Rust binary. Works with ad-hoc signing on the simulator, and an unsigned device bundle also builds | Low |

### Path B

| Item | Why | Maintenance |
|---|---|---|
| `patches/gpui-mobile-0.3.11.patch`: pin the fork at `7548ebc`, bump versions, fix one call in `src/ios/window.rs`, remove the `offscreen` feature and its vendored `gpui-pre-wgpu` | Aligns the fork with kit 0.9.0 | **Medium, and recurring.** Needed for every GPUI bump until the fork catches up |
| Swift host (`ios/App.swift`), Xcode project, display link | The fork needs a native host | Medium. About 150 lines |
| Scene lifecycle in the host | The fork's example creates its window in the app delegate. With that shape, an app built with the iOS 27 SDK traps at launch in `_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption` | Low once written |
| Own startup code (`poc_start`) instead of `ffi::run_app` | `run_app` builds `Application` privately, so the kit's icon assets can't be installed | Low. Uses public functions |
| `KEYBOARD_HOOK` in the demo | The app must call `show_keyboard()` and `hide_keyboard()` when focus changes. The fork's example does the same | Low, but every input needs it |
| `camera` and `video_player` features always on | `src/ios/platform_view.rs` uses them without a feature gate, so the crate won't compile without them | Low. A small fork patch could gate them |
