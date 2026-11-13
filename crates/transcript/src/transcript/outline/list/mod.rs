//! The outline's list: every prompt in the chat, numbered, up to two lines each, and every compaction among them, over
//! the ticks.

#[cfg(test)]
use super::{Kind, Line};
#[cfg(test)]
use protocol::model::PromptLine;

#[cfg(test)]
mod tests;

mod prompt_rows;

pub(super) use prompt_rows::{Rows, draw};
#[cfg(test)]
use prompt_rows::{changed, label};
