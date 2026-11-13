//! What drawing one event needs from the rest of the log and the model, read while they're held, so nothing is held
//! while it's drawn.

mod nearby;
mod reply;
mod settled;
mod subagent;

pub(super) use reply::{Answer, Outcome, Reply};
pub(super) use settled::{AnswerTo, Settled};
pub(super) use subagent::Subagent;
pub(crate) use subagent::{REACH, SubagentCalls, drawn_in_its_call, its_call};

#[cfg(test)]
mod tests;

mod neighbors;
#[cfg(test)]
pub(super) use neighbors::block_key;
pub(super) use neighbors::{Around, Asked, BlockAround, Closes, Opening, asks_questions, blocks_of};
pub(crate) use neighbors::{NEAR, drawn_in_row, drawn_with_its_call, retry_at};
