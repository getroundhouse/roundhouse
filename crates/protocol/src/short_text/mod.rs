//! Text made short to show it: on one line, as a title or a prompt's line in a list shows it, so a row shows it as it
//! reads, or only its start, saying whether that's all of it. Every part of the system that shortens text the same way
//! does it with these, so a hub, a client and an agent's adapter can't disagree.

#[cfg(test)]
pub(crate) mod laws;

mod shorten;
#[cfg(test)]
use shorten::hides;
pub use shorten::{one_line, start_of};
