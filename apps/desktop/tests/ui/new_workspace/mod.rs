//! The new-workspace dialog: what it makes, what the first prompt carries, what's kept when it closes, and what shows
//! once the workspace is made.

#![cfg(test)]

mod given_back;
mod kept;

mod creating;
use creating::{open_with_a_repo, prompts};
