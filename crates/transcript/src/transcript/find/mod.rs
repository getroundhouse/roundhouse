//! Finding text in a chat, from the bar `Cmd-F` opens over it, as `docs/spec/chats.md` says. The transcript holds a
//! window of the log, so it counts what the rows it holds draw itself, whenever they change, and client-core's search
//! reads the rest of the chat around each event the hub says holds the text, for it to count what the other rows
//! draw. Which match is shown, and what the bar says, is the device's own, as where the chat is scrolled to is.
//!
//! A row is counted from what draws it: one the log holds from the log, as it's drawn, and one it doesn't from the
//! stretch of the chat read around it, until it loads. Events are known by their position in the log, which is their
//! place among the agent's own, or their `seq` in a log of every event, from a hub too old to find.

mod bar;
mod count;
mod stepping;
mod stretch;
#[cfg(feature = "testing")]
mod testing;

#[cfg(test)]
mod tests;

mod query;
pub(crate) use query::Finding;
use query::is_loaded;
#[cfg(test)]
use query::{More, changed_to_count, count_held, first_match, held_to_count, says};
