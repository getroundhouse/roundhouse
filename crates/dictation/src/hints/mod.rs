//! Words and phrases a recording is likely to hear, the token graph that rewards whole matches of them while taking
//! back a partial match's boost as soon as what follows diverges, and writing a hyphenated one as it's hinted where the
//! model wrote its words another way.

mod graph;
mod spelling;
mod words;

pub(crate) use graph::{HintGraph, HintState};
pub use words::{Hint, Hints};

#[cfg(test)]
mod tests;
