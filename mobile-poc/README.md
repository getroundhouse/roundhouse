# gpui-kit on iOS: step 1 proof of concept

Does gpui-kit 0.9.0 compile and render on iOS? **Yes, on both backends tested, verified in the simulator only.** Nothing has run on a physical iPhone yet.

| | Path A | Path B |
|---|---|---|
| Backend | Zed's `gpui-pre-ios` 0.3.11 | Longbridge's `gpui-pre-mobile` at `7548ebc`, bumped |
| Compiles and renders the same screen | Yes, simulator | Yes, simulator |
| Links for `aarch64-apple-ios` | Yes, debug and release | Yes, release |
| Runs on a physical iPhone | Not attempted | Not attempted |

**Recommendation: use path A for iOS now, and keep screen code backend-neutral so path B's Android backend can be added later.** See [Recommendation](#recommendation).

Evidence is in `../.context/mobile/`: screenshots, contact sheets (`*-sheet-*.png`) and event logs (`*-event-log.txt`).

## How it was tested

Both apps compile the same UI, `shared/demo.rs`. It has 5,000 generated rows of mixed height.

`simctl` can't inject touches, so `uitest/` uses XCUITest to send real UIKit touches and on-screen keyboard taps to a headless simulator. Simulator.app was never opened; I checked this with `pgrep` after each run.

| Simulator | What ran |
|---|---|
| iPhone 18 Pro, iOS 27.2 | Full driver and screenshots, both paths |
| iPhone Air, iOS 27.0 (closest match to the real phone, on 27.0.1) | Launch screenshot only, both paths |
| iPhone 17 Pro, iOS 26.5 | Launch screenshot only, both paths |

Only debug builds ran. Release builds were linked for device but never run. Simulator timing says nothing about device performance, so none is reported.

## Build and run

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
- **Event log:** the app writes `Documents/poc.log` in its data container, because a screenshot can't show whether an event arrived.
- **Wait about 14 seconds before a screenshot.** Debug builds show a white launch screen for 2 to 3 seconds, and shots taken within about 5 seconds of launch showed a gray veil. The likely cause is the fade from that white screen, though I didn't prove it.

## What was tested

Everything here was seen on the iPhone 18 Pro simulator unless noted. Both paths behaved the same unless the row says otherwise.

| Component | Result |
|---|---|
| `MessageScroller`, 5,000 variable-height rows | Renders at top, middle and tail. A slow drag follows the finger. Flicks keep moving after release. Jump-to-latest works |
| Markdown (`TextView`) | Headings, bold, italics, inline code, links, lists and blockquotes render |
| Fenced code with syntect | Highlighted. Long lines wrap rather than scroll sideways |
| Monospace font | Needs a workaround, see below |
| `Textarea` composer | Focus, typing, backspace, auto-grow and clearing after send all work |
| `Input` (single line) | Typing works |
| `Button` | Taps fire. Send appends a row and follows the tail |
| `SidebarMenuItem` drawer rows | Render, and taps fire. `ListItem` rendered but wasn't tapped |
| Text selection (long press) | Selects a word, with handles and a Copy / Select All menu |
| Safe area and keyboard avoidance | Correct. Path A uses `window.insets()`, path B uses the host's `keyboardLayoutGuide` |
| Dark theme | Works. Path A's keyboard stayed light because nothing sets it dark. Path B's is dark because the Swift host overrides the style |
| Tapping the transcript | Dismisses the keyboard and blurs the input |

### Not verified

- Anything on a physical iPhone, including scroll feel at 120 Hz, launch time, memory and heat.
- CJK input (marked text), dictation, autocorrect and paste. The code paths exist, but nothing exercised them.
- A real hardware keyboard.
- Long-running or streaming transcripts. Only a single appended row was tested.
- Release builds at runtime.
- Path A's and path B's signed device install. `scripts/device.sh` has never been run.

## Patches and workarounds

### Both paths

| Item | Why | Maintenance |
|---|---|---|
| Explicit monospace font (`probe_mono_font` in `shared/demo.rs`) | The kit's default names a Linux font on iOS and silently falls back to the proportional system font. GPUI panics on a font it can't load, so each candidate is loaded through the platform first | Low. App code. Bundling a font would be better for production |
| `.occlude()` on the drawer panel | Without it, a tap on a row also hit the scrim behind it. This was a bug in the demo, not in either backend | None |

### Path A

| Item | Why | Maintenance |
|---|---|---|
| `patches/gpui-base-0.9.0-text-length.patch`: one method, `text_length_utf16`, on the kit's input handler | **Required.** Without it the keyboard never appears (verified). `gpui_ios` won't show it or apply edits unless it knows the document's length, and the kit never reports one. The kit's `main` branch also lacks this method | Low. 10 lines, re-applied on each kit release until upstreamed. Worth sending to gpui-kit |
| `runtime_shaders` feature on `gpui-pre-ios` | The Xcode Metal Toolchain component isn't installed, and installing it wasn't in the brief. The kit uses the same feature on desktop | None. Launch cost on a device is unmeasured. `xcodebuild -downloadComponent MetalToolchain` would allow precompiled shaders |
| `ios/project.yml` with a stub target | Only for signing: Xcode signs a stub whose executable is replaced by the Rust binary. Works with ad-hoc signing on the simulator. An unsigned device bundle also builds | Low |

### Path B

| Item | Why | Maintenance |
|---|---|---|
| `patches/gpui-mobile-0.3.11.patch`: pin the fork at `7548ebc`, bump versions, fix one call in `src/ios/window.rs`, remove the `offscreen` feature and its vendored `gpui-pre-wgpu` | Aligns the fork with kit 0.9.0 | **Medium, and recurring.** Needed for every GPUI bump until the fork catches up. The kit moved through 0.3.8, 0.3.9, 0.3.10 and 0.3.11 in four weeks (Oct 5 to Nov 2). The fork reached 0.3.10 on Nov 3, a day after the kit had moved to 0.3.11 |
| Swift host (`ios/App.swift`), Xcode project, display link | The fork needs a native host | Medium. About 150 lines |
| Scene lifecycle in the host | The fork's example creates its window in the app delegate. With that shape, an app built with the iOS 27 SDK traps at launch in `_UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption` (seen in the crash report) | Low once written. The fork's non-embedded mode also creates a window without a scene; not tested |
| Own startup code (`poc_start`) instead of `ffi::run_app` | `run_app` builds `Application` privately, so the kit's icon assets can't be installed | Low. Uses public functions |
| `KEYBOARD_HOOK` in the demo | The app must call `show_keyboard()` and `hide_keyboard()` when focus changes. The fork's example does the same | Low, but every input needs it |
| `camera` and `video_player` features always on | `src/ios/platform_view.rs` uses them without a feature gate, so the crate won't compile without them. Linking camera code may raise App Store privacy questions (unverified) | Low. A small fork patch could gate them |

## GPUI version alignment

- **Path A:** none needed. `gpui-pre-ios` 0.3.11 ships as part of the same snapshot the kit pins.
- **Path B, option 1 (used):** bump the fork to 0.3.11 and keep kit 0.9.0. This took one code line plus manifest edits, and it runs in the simulator.
- **Path B, option 2:** kit 0.8.1 pins 0.3.10, so it matches the fork unmodified. `cargo check` passes with the shared demo, but it wasn't run. It also leaves mobile one kit release behind desktop.
- **Android:** the bump doesn't cover Android. `src/android/window.rs:1405` makes the same `WgpuContext::new` call and will need the same fix. That code wasn't compiled here.

## Native embedding

- **Path B: works, verified on the simulator.** A native navigation bar and `UITextField` surround a GPUI view on one screen (`b-02-native-chrome.png`, `b-sheet-native-chrome.png`). The list scrolls inside it, the native field types with its own keyboard, and a GPUI button still responds while that field has focus. One view per app lifetime, per the fork's docs.
- **Path A: not supported as embedding.** `gpui_ios` owns the app and scene delegates, so GPUI can't go inside a native screen. The reverse does work: a native `UITextField` added on top of GPUI through the window handle takes touches and types, and GPUI still scrolls and takes taps around it (`a-sheet-native-overlay.png`). Nothing moves that field out of the keyboard's way, so the app would have to. Zed PR #62379 (native window composition, open) lists macOS, Windows and Linux work, not iOS.

## Rendering

| | Path A | Path B |
|---|---|---|
| GPU path | `gpui_apple` Metal renderer, direct. Same family as desktop macOS | `gpui_wgpu` through wgpu 30, Metal backend. No wgpu crates on path A |
| Text | `AppleTextSystem` (CoreText) | The fork's CoreText copy, adapted from Apache-2.0 GPUI code |
| Frames | `gpui_ios` owns a display link | The host drives a display link |
| Device release binary | 25.6 MB (20.0 MB stripped) | 31.4 MB (23.7 MB stripped) |

Sizes use Cargo's default release profile without LTO, and path B includes its required camera and video code. The two looked the same in screenshots.

## Licenses

Checked with `cargo metadata --filter-platform aarch64-apple-ios`, over the full resolved graph including build and proc-macro dependencies. This reads declared licenses only, and it doesn't cover the Android graph.

| Package | Declared license | Path | Assessment |
|---|---|---|---|
| `cbindgen` 0.28 | MPL-2.0 | A | Build dependency of `gpui-pre-apple`; not shipped in the app |
| `option-ext` 0.2 | MPL-2.0 | A, B | Reached through the `rust-embed-impl` proc macro; compile time only |
| `self_cell` 1.3 | Apache-2.0 OR GPL-2.0-only | B | Elect Apache-2.0. Comes through `cosmic-text` and `gpui-pre-wgpu` |
| `gpui-pre-mobile` 0.1 | GPL-3.0-or-later OR AGPL-3.0-or-later OR Apache-2.0 | B | Elect Apache-2.0. Confirmed in `Cargo.toml`, the README and crates.io |

**No package in either graph is GPL or AGPL only, and no Zed GPL crate appears.** Everything else is permissive.

### Does anything beneath path B force copyleft?

I found nothing that does, with limits:

- **Imports:** the fork imports no Zed GPL crate. Its one `theme` import is a local module.
- **`gpui-pre-wgpu`:** declared Apache-2.0, with `LICENSE-APACHE` included. The fork's vendored copy adds 158 lines to crates.io 0.3.10 with no license changes. This build uses the stock crates.io 0.3.11 instead.
- **`src/ios/text_system.rs`:** says it is adapted from the macOS text system, and 247 of its 359 long lines match Apache-2.0 `gpui-pre-macos`.
- **License history:** the first commit was MIT and Apache. The next commit, the same day, by the then sole author, changed it to the current three-way license. I saw no CLA, so later contributions are assumed to come under the same terms. This isn't legal advice.
- **Not done:** I didn't compare the fork against Zed's GPL source, because the rule says not to read it. A code-scanning tool or a lawyer should do that before shipping.

Flags for later:

- The fork's example app icons are Zed's logo (commit `68380a3`). Apache-2.0 grants no trademark rights, so don't copy them.
- The Android example bundles `NotoColorEmoji.ttf`, whose license I didn't check.
- `src/android/` contains AccessKit code and carries its copyright notices, which need keeping.

## Return key and hardware shortcuts

**The limitation still applies to path A.** `src/ios` in `gpui-pre-ios` 0.3.11 is identical to Zed's `main` as of this check.

| | Path A | Path B |
|---|---|---|
| On-screen Return in `Textarea` | Inserts `\n`. No `PressEnter`, no key event (verified) | Same (verified) |
| On-screen Return in `Input` | Newline stripped. No `PressEnter` (verified) | Same (verified) |
| `submit_on_enter(true)` | Doesn't submit | Doesn't submit |
| Return key label | Follows `input_action`: Send, Done (verified) | Always "return" (verified) |
| Backspace | Text edit | `backspace` key event (verified) |
| Autocorrect and capitalization settings | Honored in code | Hardcoded off in code |
| Hardware shortcuts | **No code path:** no `pressesBegan`, `UIKeyCommand` or `KeyDown` in the crate. From source only | A `gpui_ios_handle_key_event` function exists, but nothing in the crate calls it. A host would have to forward key presses. From source only |

XCUITest's `typeKey` isn't in the iOS SDK headers, so hardware keys need a person to test them. The `keystroke:` lines in `poc.log` will show what arrives.

For the composer, the app needs its own rule, such as a Send button (works today) or treating a newly inserted trailing newline as send. Shift+Return can't be told apart on these paths.

## Effect on later steps

- **Sharing transcript code with desktop:** promising. All 12 modules of the desktop prototype, since deleted, passed `cargo check` for the iOS simulator target, including its custom transcript and terminal, with no changes. **That is compile only; none of it has run on iOS.** Terminal code would also hit iOS's ban on spawning processes, so it needs a remote backend.
- **Same UI code on both backends:** `shared/demo.rs` needed only the keyboard hook and the host's safe-area handling to differ.
- **Phone-shaped app:** the kit's `MessageScroller`, `TextView` and inputs worked without layout fixes at phone width.
- **Minimum iOS version needs a decision.** On the iOS 26.5 simulator, GPUI rejects every named system font when loading it, on both paths ("Failed to read traits"), leaving only the system font, so code is proportional. iOS 27.0 and 27.2 are fine, and the phone is on 27.0.1. Bundling a monospace font might get around this; untested.
- **Launch screen:** the default white one needs a dark color.
- **VoiceOver:** neither iOS backend has accessibility code, from source. GPUI elements are also invisible to XCUITest, so UI tests have to use coordinates. Path B has TalkBack on Android through AccessKit.
- **Dialogs and files on path A:** `prompt` returns `None`, and file pickers are reported as unimplemented. Not exercised.

## Recommendation

**Use path A for iOS now. Add path B's Android backend when Android starts.**

Why path A:

- It tracks the exact GPUI snapshot the kit pins, with no alignment work. GPUI moves every week or two, so path B's bump would keep recurring.
- It needs no Swift host. The app is plain Rust, and the only patch is one small method that could go upstream.
- It has more native text behavior: return key labels, autocorrect settings, automatic keyboard on focus, and `window.insets()` for safe area and keyboard.
- It uses the same Metal renderer family as desktop macOS and produces a smaller binary.
- It's Apache-2.0 only, so there's no license election to defend.

What this gives up, and what would change my mind:

- **Native embedding.** If screens need native navigation or a native composer around GPUI, path B is the only one that does that today.
- **One backend everywhere.** Path A plus Android means two backends, though the screen code barely noticed the difference. If keeping one backend matters more than version lag, use path B for both.
- **Maturity.** Path A is weeks old. Longbridge reports testing a chat UI on path B on both platforms (their claim; not checked here). If path A fails on the real phone, path B is the fallback.

The on-device test should compare both before this is final.

### What Android would need on path B (not set up)

- The `aarch64-linux-android` Rust target, the Android SDK and NDK, `cargo-ndk`, and Gradle.
- The one-line `WgpuContext::new` fix at `src/android/window.rs:1405`, plus whatever else compiling against 0.3.11 turns up.
- A host activity extending `GpuiInputActivity` for IME, and `GpuiPathPicker` for file prompts.
- A shared insets shim. `window.insets()` isn't implemented on the fork's iOS or Android windows, from source.
- A license check of the Android dependency graph.

## What the user needs to do with the iPhone

The phone is an iPhone Air on iOS 27.0.1. `devicectl` reports it as paired, unavailable, with **Developer Mode disabled**.

1. **Connect it:** plug in the iPhone by cable, unlock it, and accept any trust prompt.
2. **Turn on Developer Mode:** go to Settings > Privacy & Security > Developer Mode, turn it on, restart, and confirm.
3. **Choose a signing team:**
   - Two teams have certificates on this Mac. Only one has an Apple Development certificate, and one of its two copies is revoked. The other has only a Developer ID certificate, which can't sign for an iPhone, so Xcode would need to create a development one.
   - The specific team IDs and the device ID are in `.context/mobile/device-setup.txt`, which isn't committed. List them with `security find-identity -v -p codesigning` and `xcrun devicectl list devices`.
4. **Install it.** This lets Xcode manage signing, so it can register the device and bundle ID and create a profile in that team. Nothing has been run yet.
   ```sh
   TEAM_ID=<team> DEVICE=<device-id> scripts/device.sh a
   TEAM_ID=<team> DEVICE=<device-id> scripts/device.sh b
   ```
   If a bundle ID is already taken, add `BUNDLE_ID_SUFFIX=.yourname`. If iOS blocks the first launch, trust the developer under Settings > General > VPN & Device Management.
5. **Try these on each path:**
   - Scroll fast through the list: is it smooth, and does it feel like iOS?
   - Type in the composer, including autocorrect, dictation and a CJK keyboard.
   - Long-press text and copy it. Paste into the composer.
   - Connect a hardware keyboard, try Cmd+C and Return, and see what `poc.log` records.
   - Note the launch time and any delay before the first frame.

## Layout

```text
shared/demo.rs      The screen under test, used by both paths
path-a/             gpui_ios app: Rust only, plus an Xcode stub for signing
path-b/             gpui-pre-mobile app: Rust static library and Swift host
patches/            The diffs we maintain. patches/build/ is generated and ignored
scripts/            prepare-patches.sh, device.sh
uitest/             XCUITest driver for touches and keyboard taps
```
