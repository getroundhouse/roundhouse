//! A workspace's terminal in a GPUI view. It paints what client-core's `OpenTerminal` copies of what the terminal shows
//! as a grid of glyphs, and gives it the keys, pastes and turns of the wheel, which it sends on as the program reads
//! them, by the modes the program set. "Terminals" in `docs/architecture.md` says who keeps what.
#![warn(clippy::wildcard_enum_match_arm)]

mod grid;
mod keys;
mod view;

pub use view::{KEY_CONTEXT, SHOWN_ID, TerminalView, bind_keys};
