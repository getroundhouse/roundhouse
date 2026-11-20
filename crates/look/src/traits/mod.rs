//! What a client is while it runs, as "Each client" in `docs/spec/look.md` says: its reach, which chooses the
//! foundations' values, and its input, which chooses how components respond. A layout's width is the client's own.

mod input;
mod reach;

pub use input::Input;
pub use reach::Reach;
