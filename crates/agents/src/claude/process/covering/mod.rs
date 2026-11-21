//! Saying what the agent is doing while it can't answer what was said out loud: the cover line, which the spec's
//! conversation mode describes. Something said while the agent is on a turn finds it busy, and something said while it
//! isn't finds it thinking about what was said. Each opens a wait, and the stand-in writes the line as soon as it's
//! said, which is sent once the agent's time to answer is up, unless it has answered by then: written words of its
//! own, or ended the turn that took what was said. Thinking, calls and its subagents' words don't count, since it's
//! still busy. The wait closes once it answers, or the mode goes off.

mod lately;
mod stand_in;
#[cfg(test)]
mod tests;
mod waits;

pub(super) use stand_in::Starting;
pub(super) use waits::Covering;
