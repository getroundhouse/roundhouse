//! What drawing one event needs from the rest of the log and the model, read while they're held, so nothing is held
//! while it's drawn.

mod failures;
mod heard;
mod nearby;
mod reply;
mod settled;
mod subagent;

pub(crate) use failures::redrawn_with;
pub(super) use failures::{Started, Tries, Try};
pub(super) use heard::Heard;
pub(super) use reply::{Answer, Outcome, Reply};
pub(super) use settled::{AnswerTo, Settled};
pub(super) use subagent::{Names, Subagent};

#[cfg(test)]
mod tests;

mod neighbors;
#[cfg(test)]
pub(super) use neighbors::block_key;
pub(super) use neighbors::{Answers, Around, Asked, BlockAround, Closes, Opening, asks_questions, denied};
pub(crate) use neighbors::{NEAR, blocks_of, drawn_in_row, drawn_with_its_call, retry_at};
