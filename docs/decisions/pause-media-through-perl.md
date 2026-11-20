# The app pauses what's playing with a library of its own, which `/usr/bin/perl` loads

**Status:** Decided.
**First recorded:** 2026-11-20, in #1628.

macOS has no public way to pause another app's music or video. MediaRemote, the private framework that says what a Mac is playing and sends play and pause, answers only Apple's own programs. `/usr/bin/perl` is one of them, and loads a library it's given. So Pause media while dictating starts perl with `now-playing`, a library of the app's own, which pauses the Now Playing app if it's playing, and plays it again once dictating stops. [MediaRemote](../reference/media-remote.md) has what was found of it.

## Chosen over

- **mediaremote-adapter,** the open-source adapter other apps run through perl in the same way. It's about 2,700 lines of Objective-C and Perl, of which the app would use two commands, kept in step by hand. And it takes four runs of perl a dictation rather than one.
- **The same library in C,** compiled by a build script. It would be the repository's first C, which no lint or test covers.
- **A crate Cargo builds as a library.** Cargo builds one for a crate that depends on it, but in a folder whose name changes with the build, and tells that crate nothing of where, so the app couldn't find it.

## What it costs

- Apple can close the route in any update, by no longer answering perl, by stopping perl loading a library, or by taking perl out of macOS, as it took Python 2 out in macOS 12.3. Then the switch pauses nothing, and says nothing.
- `now-playing`'s Mac code uses no crates, since its build script compiles the crate a second time with `rustc` alone, as the library perl loads. So it lays out MediaRemote's callbacks itself, in unsafe code. Cargo compiles the part perl runs only into the crate's tests, where its lints and tests run.
- A Mac's package carries the library beside the binaries, which `tools/build.sh` puts there, and `tools/package/package.sh` checks is signed with them.
- Windows has a public API for media sessions, so it needs no helper. `now-playing` pauses through it in the app, by the same rule for playing again that perl follows on a Mac.
