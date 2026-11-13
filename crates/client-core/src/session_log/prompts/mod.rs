//! Whether the agent has taken a recorded prompt yet, and so where the prompt is drawn.

#[cfg(test)]
use super::SessionLog;
#[cfg(test)]
use protocol::model::Settlement;

#[cfg(test)]
mod tests;

mod settlement;
pub use settlement::{PromptAt, PromptState, Settled};
