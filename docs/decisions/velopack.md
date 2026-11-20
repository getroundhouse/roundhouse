# Velopack packages, installs and updates the app

**Status:** Decided.
**First recorded:** 2026-11-09, in #215.

Velopack packages the app and makes its updates for macOS, Windows and Linux, so one tool covers all three. An update downloads only what changed.

## Chosen over

- **Sparkle.** It signs its updates, but covers only the Mac.
- **An updater of our own.** It would have to redo what Velopack does on Windows.

## What it costs

The app works around several things Velopack does:

- It checks a package only against a checksum in the update feed. So the feed is signed, and the app checks the signature before trusting anything in it.
- On Windows it kills every process running from the install directory before it updates. So [the hub runs from a copy outside that directory](hub-runs-from-its-own-copy.md).
- On Windows it installs the app in `%LOCALAPPDATA%\Roundhouse`, and Windows ignores case. So the hub's data is in `roundhouse-data`, not `roundhouse`.
- On Linux the only package it updates is an AppImage, so [the Linux app is one](linux-appimage.md).
- On Linux it asks for an administrator's password when the user can't write to the AppImage's folder, so the app says it can't update there instead. And it would keep downloads in `/var/tmp`, which every user shares, so the app has it use the data directory.
- Each app ships Velopack's own prebuilt updater, whose crates the license check can't see. Velopack itself is under MIT.
