//! The prompts each session's agent has yet to take or drop, kept on the session's record.

#[cfg(test)]
use super::Store;

#[cfg(test)]
mod tests;

mod prompt_rows;
pub(super) use prompt_rows::note_prompts;
