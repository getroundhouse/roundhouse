//! Every prompt in the session, as a line each, for finding one in a long chat, and every compaction, for marking where
//! the agent compacted. The log holds only a window of events, so these come from the hub's lists and from every
//! prompt and compaction the log has been given. Between them they miss none: the
//! subscription sends everything from where it starts, and every prompt before that was recorded before the list was
//! asked for, which is once the subscription has started. Each list is asked for once, and a hub that fails to answer
//! is asked again on a new connection, as a page is, while the log follows the session either way. A hub can be new
//! enough to list prompts and too old to list compactions, so each list goes its own way.
//!
//! A chat's rows go by the agent's own events, so a prompt or a compaction is drawn at its place among them. A hub that
//! can't read by place gives the whole log, and there each is at its `seq`.

#[cfg(test)]
use super::{Position, SessionLog};
#[cfg(test)]
use protocol::event::SessionEvent;
#[cfg(test)]
use protocol::model::{CompactionLine, PromptLine, Seq};

#[cfg(test)]
mod tests;

mod prompts;
pub(super) use prompts::Outline;
