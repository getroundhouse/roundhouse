//! What changed within the lines of a diff, word by word, which both diffs mark so a small edit to a long line stands
//! out. A run of removed lines and the run of added lines that took its place are compared as one text, so words that
//! only moved to another line, as when a paragraph is wrapped again, aren't marked. Comparing each removed line with
//! the added line at its place instead marks the words that moved, matches common words between lines that aren't
//! related, and marks nothing on the lines one run has more of.

#[cfg(test)]
use std::ops::Range;

#[cfg(test)]
mod laws;
mod split;
#[cfg(test)]
mod tests;

mod compare;

pub use compare::{LineChange, word_changes};
#[cfg(test)]
use compare::{compared, shared_start};
