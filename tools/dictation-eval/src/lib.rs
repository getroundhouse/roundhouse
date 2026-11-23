//! Scoring dictation's hint words against made-up cases. Each case says what's around a field someone dictates
//! into, what they then say, and the terms in it they'd want spelled right. Its score is which of those terms the
//! field hints, as the engine takes the hint words, and how many of the words favour none of those terms. A test holds
//! every case's score to the baseline checked in beside them, so a change to which words are hinted shows what it
//! gained and lost.

#![warn(clippy::wildcard_enum_match_arm)]

mod built;
mod case;
mod score;
mod scores;

pub use case::{Case, FieldKind, Invalid, Term};
pub use score::Score;
pub use scores::{EvalError, Scores};

#[cfg(test)]
mod tests;
