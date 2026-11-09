# Why GPUI and gpui-kit

Why the apps are built on GPUI and gpui-kit, why the transcript is built on the kit's views, and why the iOS app runs on Zed's iOS backend. How code that uses them is written is in `docs/gpui.md`.

## GPUI

The desktop app is native Rust on GPUI, Zed's UI framework, rather than a web view.

- **It meets the bars in `docs/spec/speed.md` at 120 Hz.** A web view started and opened diffs more slowly, and WKWebView caps animation near 60 Hz, with no public API to change that.
- **It's Rust end to end,** like the hub, the worker and `client-core`.
- **It reaches a phone,** so the iOS app shares the transcript with the desktop app. See iOS, below.
- **It has AccessKit integration,** for VoiceOver.
- **Its own crates are Apache-2.0.** Zed's other crates are GPL-3.0, so they're out.

**What it costs:**

- Its mobile backends are experimental.
- It's pre-1.0, and the kit pins exact snapshots of it, so an upgrade can change the API.
- There's more code to own than with a web view. Neither GPUI nor the kit has a terminal, so the app's terminal builds on `alacritty_terminal`.

## gpui-kit

Longbridge's component kit for GPUI, used in their shipping Longbridge Pro app.

- **It has most of what the app needs:** messages, a message scroller, diffs, a sidebar, docks, a command palette, settings, native menus, variable-height virtual lists, a code editor, and Markdown.
- **It runs UI tests headless,** so the app's tests click and type without a window on screen.
- **It's Apache-2.0.**

## The transcript

The transcript is the kit's `MessageScroller` and `TextView`, plus a small windowed element for large tool output, in `crates/transcript`.

- **It meets every bar,** and on real content it nearly matches a custom renderer.
- **Giant rows are fine.** A windowed element of about 70 lines flings through a multi-megabyte row within the bars.
- **Markdown, tables, code highlighting, links and selection across messages come with it.** A custom renderer would also need bidi, accessibility and find.
- **It's the same code on iOS,** where the kit's `MessageScroller` scrolls at 120 Hz.

**What it costs:** at the extreme it's less smooth than a custom renderer, with an occasional frame over 25 ms, and it uses more memory and starts more slowly. The renderer sits behind `crates/transcript`, so replacing it touches nothing else.

## iOS

The iOS app, `apps/mobile-ios`, is GPUI throughout, on Zed's iOS backend, `gpui-pre-ios`, and shares the transcript with the desktop app. `mobile-poc/` holds the proof of concept that chose it.

- **It behaves like an iPhone app where it counts.** It scrolls at 120 Hz, and selection, copy and paste, and editing with the on-screen keyboard work as in other iOS apps, including moving the cursor from the space bar.
- **It's released with GPUI,** at the version the kit pins, so an upgrade needs no porting.
- **It's Rust only and Apache-2.0 only,** with no native host app to keep in step.
- **Screens don't depend on the backend.** The same screen code ran on both backends tried, so switching backends, or adding Android, doesn't mean rewriting them.

Why not the alternatives:

- **Longbridge's `gpui-pre-mobile`** can place GPUI inside native iOS screens, and covers Android too. On iOS, though, it handles the keyboard as a stream of key presses, so moving the cursor from the space bar doesn't work, and it trails GPUI's releases, so each upgrade needs porting. It's the fallback if Zed's backend fails.
- **A native SwiftUI app** would mean writing every screen twice, including the transcript, the hardest of them.
- **Native screens around a GPUI view** would rule out Zed's backend, which owns the whole app and can't be embedded, and GPUI's own scrolling and text input hold up without them.

**What it costs:**

- **No VoiceOver.** Neither iOS backend has accessibility code, unlike GPUI on the Mac.
- It's experimental, and young.
- Native views can only sit on top of GPUI, not around it, so a native navigation bar or composer would mean switching backends.
- It needs two changes the backend doesn't carry. The keyboard never appears for an input that reports no document range, as the kit's don't, and the display link has to ask for 120 Hz, or an iPhone holds it to 60. `third_party/gpui-pre-ios/` is the backend with both, and `apps/mobile-ios/README.md` says how to carry them to a new release.
