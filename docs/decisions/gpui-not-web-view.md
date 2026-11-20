# The desktop app is native Rust on GPUI, not a web view

**Status:** Decided.
**First recorded:** 2026-11-06, in #78.

The desktop app is native Rust, drawn with GPUI, the UI framework Zed is built on.

- **It meets the bars in `docs/spec/speed.md` at 120 Hz.**
- **It's Rust end to end,** like the hub, the worker and `client-core`.
- **It reaches a phone.** [The iPhone app runs on Zed's iOS backend for GPUI](zed-ios-backend.md), so it shares the transcript with the desktop app.
- **It has AccessKit integration,** for VoiceOver.
- **Its own crates are under Apache-2.0,** which [the license rule](no-strong-copyleft.md) allows.

## Chosen over

- **A web view.** It started more slowly, and opened diffs more slowly. WKWebView, macOS's web view, also caps animation near 60 Hz, and has no public API to change that.

## What it costs

- **Its mobile backends are experimental.**
- **It's pre-1.0,** and gpui-kit pins exact snapshots of it, so an upgrade can change the API.
- **There's more code to own than with a web view.** Neither GPUI nor gpui-kit has a terminal, so the app's terminal is built on Ghostty's terminal core, `libghostty-vt`.
