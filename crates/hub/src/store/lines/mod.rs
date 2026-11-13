//! Each prompt and compaction recorded in a chat, as one line, for the lists that don't need whole events: a chat's
//! outline, and what a workspace's title is worked out from.

#[cfg(test)]
use super::lock;
#[cfg(test)]
use protocol::model::{CompactionLine, PromptLine, Seq};

#[cfg(test)]
mod tests;

mod transcript_lines;
