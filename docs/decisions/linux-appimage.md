# The Linux app is an AppImage

**Status:** Decided.
**First recorded:** 2026-11-12, in #680.

The Linux app ships as an AppImage: one file that runs on most distros without installing. It's the one package format that [Velopack](velopack.md) updates on Linux, so the app updates itself there as it does on a Mac.

## Chosen over

- **Flatpak.** Its sandbox would keep the app from the host's git, shells and agents, which the app runs all the time.
- **A `.deb` or `.rpm`.** It would hand the app's updates to a package manager.

## What it costs

- **It's the one exception to the license rule.** Nothing else that ships is under GPL, AGPL or LGPL. An AppImage starts with the AppImage runtime, a program that mounts the app's files and starts the app. Every AppImage runtime uses FUSE, and this one has libfuse built in, which is under LGPL-2.1. The runtime is a separate program, under MIT and unchanged from upstream. So the LGPL covers only the runtime, and none of our code links libfuse. The app's notices have to say where the sources of the runtime and of libfuse are.
- **The app's files exist only while the app runs.** So a hub can't run from them, and [runs from its own copy](hub-runs-from-its-own-copy.md).
