//! The outline: a short tick at the right of the chat for each prompt, with the one being read in white, so a long chat
//! shows how far it goes and where you are in it, and a dot for each compaction, where the agent swapped what it held
//! for a summary. Resting the pointer on them lists the prompts and compactions over them, and clicking one jumps to
//! it. The ticks themselves aren't clicked, so they can stay small.

#[cfg(test)]
use protocol::model::{CompactionLine, PromptLine};

mod hover;
mod list;
#[cfg(test)]
mod tests;

mod ticks;
pub(super) use ticks::Outline;
use ticks::{Jumps, Kind, Line};
#[cfg(test)]
use ticks::{MOST_TICKS, Tick, being_read, look, shown};
