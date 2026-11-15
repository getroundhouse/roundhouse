//! `get_diff_comments`: the review comments the person has left on lines of the workspace's diff, which the hub keeps,
//! for the agent to read when the person mentions them. They stay on the diff until the person deletes them.

#[cfg(test)]
mod tests;

mod reading;
#[cfg(test)]
use reading::describe;
pub(super) use reading::{NAME, call, definition};
