# The iPhone app is GPUI throughout, on Zed's iOS backend

**Status:** Decided.
**First recorded:** 2026-11-20, in #237.

The iPhone app, in `apps/mobile-ios`, draws every screen with GPUI, on `gpui-pre-ios`, the iOS backend that Zed publishes. It shares the transcript with the desktop app.

- **It behaves like an iPhone app where it counts.** It scrolls at 120 Hz. Selecting, copying, pasting and editing with the on-screen keyboard work as in other iOS apps, including moving the cursor from the space bar.
- **It's released with GPUI,** at the version gpui-kit pins, so an upgrade needs no porting.
- **It's Rust only and Apache-2.0 only,** with no native host app to keep in step.
- **Screens don't depend on the backend.** The same screen code ran on both backends tried, so switching backends, or adding Android, doesn't mean rewriting the screens.

## Chosen over

- **Longbridge's `gpui-pre-mobile`.** It can place GPUI inside native iOS screens, and it covers Android too. But on iOS it handles the keyboard as a stream of key presses, so moving the cursor from the space bar doesn't work. It also trails GPUI's releases, so each upgrade needs porting. It's the fallback if Zed's backend fails.
- **A native SwiftUI app.** Every screen would be written twice, including the transcript, which is the hardest of them.
- **Native screens around a GPUI view.** Zed's backend owns the whole app and can't be embedded, so this would rule it out. GPUI's own scrolling and text input hold up without native screens.

## What it costs

- **No VoiceOver.** Neither iOS backend has accessibility code, unlike GPUI on the Mac.
- **The backend is experimental, and young.**
- **Native views can only sit on top of GPUI, not around it.** So a native navigation bar or composer would mean switching backends.
- **It needs changes that GPUI's releases don't carry.** So the app builds the backend, and the text system under it, [from copies in the repo](gpui-kit-copy.md), and `third_party/README.md` lists each change.
