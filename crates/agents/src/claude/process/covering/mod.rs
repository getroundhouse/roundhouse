//! Saying what the agent is doing while it can't answer a prompt said out loud: the cover line, which the spec's
//! conversation mode describes. A prompt said out loud while the agent is on a turn finds it busy, and one said while
//! it isn't finds it thinking about what was said. The stand-in writes the line as soon as it's said, and it's reported
//! once the wait for the agent to answer is up, unless the agent has started answering by then: written words of its
//! own since the prompt was sent, as the tail's rules for the line say, or ended its turn. Thinking, calls and its
//! subagents' words don't count, since the agent is still busy. Only the newest prompt said out loud is covered.

mod lately;
mod lines;
mod stand_in;
#[cfg(test)]
mod tests;

pub(super) use lines::Covering;
pub(super) use stand_in::Starting;
