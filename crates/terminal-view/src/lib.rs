//! A workspace's terminal in a GPUI view. It paints what client-core's `OpenTerminal` copies of what the terminal shows
//! as a grid of glyphs, and gives it the keys, pastes and turns of the wheel, which it sends on as the program reads
//! them, by the modes the program set, and what the pointer does, which selects, and asks it for what's selected to
//! copy. The copy is client-core's, and this view only draws it and passes input on, as `docs/principles/layers.md`
//! says of every view.
#![warn(clippy::wildcard_enum_match_arm)]

mod color;
mod fitting;
mod grid;
mod keys;
mod view;

pub use fitting::Room;
pub use view::{KEY_CONTEXT, SHOWN_ID, TerminalView, bind_keys};
