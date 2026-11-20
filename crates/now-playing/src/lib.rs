//! Pausing what the app the keyboard's media keys control plays while something is dictated, if the person has chosen
//! it, and playing it again once dictating stops, as `docs/spec/settings.md` says, on each platform that can. Each of
//! [`pause`] and [`play_again`] waits on another process, so `dictation` calls them on a thread of its own.
//!
//! Windows pauses its current media session through its public API for them. A Mac pauses its Now Playing app through
//! `MediaRemote`, the private framework that says what's playing, which answers only Apple's own programs.
//! `/usr/bin/perl` is one of them, and loads a library it's given, so [`pause`] starts perl with this crate as a
//! library, and has it call `roundhouse_pause_while_held`, which holds the pause until [`play_again`].
//!
//! So on a Mac this crate is built twice. Cargo builds it for `dictation`, as the part that starts perl. And the build
//! script compiles it with `rustc` alone, as the library perl loads, setting `perl_library`, which keeps each build to
//! its own part. Alone, because Cargo would build the library in a folder whose name changes with the build, which the
//! app couldn't find. So the part perl runs uses no crates, and Cargo compiles it only into this crate's tests, where
//! its lints and tests run. `docs/reference/media-remote.md` has what it depends on of `MediaRemote`, and
//! `docs/decisions/pause-media-through-perl.md` why it's done this way.

#![warn(clippy::wildcard_enum_match_arm)]

#[cfg(not(any(target_os = "macos", windows)))]
mod elsewhere;
#[cfg(all(target_os = "macos", not(perl_library)))]
mod mac;
#[cfg(any(perl_library, all(target_os = "macos", test)))]
mod perl;
#[cfg(windows)]
mod windows;
// What perl and Windows decide, which is tested everywhere.
#[cfg(any(perl_library, windows, test))]
mod now_playing;

#[cfg(not(any(target_os = "macos", windows)))]
pub use elsewhere::{PAUSES, Paused, pause, play_again};
#[cfg(all(target_os = "macos", not(perl_library)))]
pub use mac::{PAUSES, Paused, pause, play_again};
#[cfg(windows)]
pub use windows::{PAUSES, Paused, pause, play_again};
