//! What a terminal shows, kept by the worker from what the program in it writes: the rows a client is sent of it by
//! number, as `protocol::terminal::TerminalPicture` says, a word's ends and the text a client selects, and what the
//! program reads for what a client pressed. A client holds no copy of its own, and shows the rows it's sent.
//!
//! It does no IO and has no UI. Only the worker builds it, and tests, such as the fake hub's, which makes a terminal's
//! pictures from its bytes as a worker would.
#![warn(clippy::wildcard_enum_match_arm)]

mod mirror;

pub use mirror::{Mirror, MirrorError, QUIET_BEFORE_COMPRESSING, Tracked};
