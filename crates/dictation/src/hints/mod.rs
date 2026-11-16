//! Words and phrases a recording is likely to hear, and the token graph that rewards whole matches of them while
//! taking back a partial match's boost as soon as what follows diverges.

mod graph;
mod words;

pub(crate) use graph::{HintGraph, HintState};
pub use words::Hints;

#[cfg(test)]
mod tests;
