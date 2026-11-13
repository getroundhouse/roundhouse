//! What a subagent has done lately, which its call shows while it works: its newest calls, each with the text it wrote
//! just before it and what came back, then what it's written since. Every change to a subagent's record goes to every
//! client, so only a few calls are kept, and each text is cut short. A call's input is kept whole, as the record's
//! newest call always has been, so its line says what it did, such as how many lines a write wrote. A write's input
//! holds the whole file, so that's what can still make a record large.

#[cfg(test)]
mod tests;

mod activity;
pub(super) use activity::add;
#[cfg(test)]
use activity::{CALLS_KEPT, RESULT_KEPT, TEXT_KEPT};
