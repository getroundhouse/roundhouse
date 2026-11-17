//! What a subagent has done lately, which its call shows while it works: its newest calls, each with the text it wrote
//! just before it and what came back, then what it's written since. Every change to a subagent's record goes to every
//! client, so only a few calls are kept, and each text is cut short. A call is kept whole, so its line says what it
//! did, such as how many lines a write wrote. A write holds the whole file, so that's what can still make a record
//! large.

#[cfg(test)]
mod tests;

mod adding;
pub(super) use adding::add;
#[cfg(test)]
use adding::{CALLS_KEPT, RESULT_KEPT, TEXT_KEPT, kept};
