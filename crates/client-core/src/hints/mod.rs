//! Gathering the words a dictated prompt or answer is likely to say from what the client already holds of the
//! workspace, repository, open session and question, for each field that can be dictated into.

mod collect;
mod english;
mod field;
mod prose;

use collect::HintCollector;
pub use field::{Around, Field, HintWord, Workspace};

#[cfg(test)]
mod tests;
