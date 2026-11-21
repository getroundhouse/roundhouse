//! A terminal's rows by number: counting them as they scroll into its history and out of it, and saying what a row
//! holds, so a row of history can be asked for by its number, as `protocol::worker::TerminalRows` says.

#[cfg(test)]
mod tests;

mod counting;
mod fitting;
mod reading;
mod showing;
mod text;

pub(super) use counting::Counting;
pub use fitting::Tracked;
pub(super) use showing::Picturing;
