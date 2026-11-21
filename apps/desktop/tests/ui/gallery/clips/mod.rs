//! Short videos of the real app, drawn from made-up data as the gallery's screens are, a frame at a time, to show
//! what happens over time, such as a bug that a picture can't show. They're to watch, not to check, so each is a test
//! that's run by name, as `docs/reference/commands.md` says, and none runs with the others.
//!
//! Each goes in `.context/clips/<name>/` at the repo's root: the video, `<name>.mp4`, as `clips/video.rs` says, with
//! the pointer drawn over it, as `clips/pointer.rs` says, what was done in it and when, in `shots.txt`, and in
//! `frames/` each frame that differs from the one before, by its number, as the app drew it, for looking at closely.

#![cfg(test)]

mod chat;
mod finding;
mod folder;
mod pointer;
mod video;
