# Mobile proof of concept plan

**Goal:** find out whether the transcript can run well on a real iPhone using GPUI, with code shared with the desktop app. Results go in `README.md` in this directory. This work doesn't block milestone 1.

**Status:** step 1 is finished, and the on-device part is waiting on iPhone setup. gpui-kit compiled and rendered on both backends in the simulator, with one small required patch on `gpui-pre-ios`. The current recommendation is `gpui-pre-ios` for iOS, adding `gpui-pre-mobile` when Android starts. See `README.md`.

**What step 1 found that affects later work:**
- **Composer:** Return never fires `PressEnter` on either backend, so `submit_on_enter` doesn't work and the composer needs its own send rule.
- **Minimum iOS needs a decision:** on the iOS 26.5 simulator, GPUI rejected every named system font, so code rendered proportionally. iOS 27.0 and 27.2 were fine.
- **VoiceOver:** neither iOS backend has accessibility code, from reading the source.
- **Terminal:** iOS can't spawn processes, so the terminal needs a remote backend. This fits the hub and worker model.
- **Licensing:** no GPL-only or AGPL-only package in either dependency graph. Run a code scanner before shipping `gpui-pre-mobile`, and don't copy its example icons, which are Zed's logo.

## The two backends

| | Zed's `gpui-pre-ios` | Longbridge's `gpui-pre-mobile` |
|---|---|---|
| **License** | Apache-2.0 | `GPL-3.0-or-later OR AGPL-3.0-or-later OR Apache-2.0`, electing Apache-2.0 |
| **GPUI version** | 0.3.11, same as gpui-kit 0.9.0 | 0.3.10 on main when checked |
| **Android** | No | Yes |
| **Embeds in a native screen** | No API found. An open Zed PR on native window composition may be relevant | Documented |

Check the dependency tree too, because electing Apache-2.0 only helps if nothing underneath forces copyleft. GitHub's API labels both `gpui-mobile` repos AGPL, which is misleading.

## Steps

1. **Check it compiles.** Build a small gpui-kit screen on each backend, in the simulator and then on the phone. If the kit needs substantial changes or a fork, stop and reassess.
2. **Share the transcript code.** Use the real `client-core` and `transcript` crates, to show how much can really be shared.
3. **Build a phone-shaped app.** A workspace list, transcript, composer and read-only diff, not a shrunken desktop layout. It talks to `test-server/` over Wi-Fi, so the server needs to bind beyond `127.0.0.1`.
4. **Test on the iPhone.** The simulator can't prove device performance.

## On-device tests

| Test | Target |
|---|---|
| Open a session | p95 at or under 500 ms |
| Scroll the 15k session while streaming | p99 frame at or under 18 ms, no frame over 250 ms |
| Memory | Not killed by iOS with the largest messages loaded |
| 10 minutes of streaming | Record heat and battery |

**Hand checks by the user, about 20 minutes:**
- Flick scrolling and bounce next to Safari. GPUI handles scrolling itself rather than using iOS's native scrolling, which is the biggest risk.
- Text selection and copy.
- Keyboard: autocorrect, dictation, Japanese input and the Return key.
- VoiceOver.
- Resuming from the background.

## How to decide

| Result | What to do |
|---|---|
| Tests pass and it feels right | Use GPUI on mobile |
| Fast, but scrolling or text feels wrong | Try to fix it, or use a native shell hosting a GPUI transcript view |
| Won't compile, crashes or freezes | Build a native SwiftUI client on the same protocol |

Desktop isn't affected by any of these outcomes, because the protocol is the boundary between clients.

**Native shell** means a normal iOS app built with Apple's UI tools, with native navigation and composer, where GPUI draws only the transcript and diff. It gets real iOS keyboard and navigation behavior, but it means Swift and Rust with a bridge, and a second shell for Android.

## Android

Out of scope for now. If `gpui-pre-mobile` holds up on iOS, one backend could cover both phones. The alternatives are writing our own backend on `gpui_wgpu`, or a native Kotlin client on the same protocol. Zed has no Android backend, and we found no sign of one in progress.

## Also out of scope

Push notifications, the real hub, the relay and App Store submission.
