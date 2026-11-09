//! Whether the agent has taken a recorded prompt yet.

use protocol::event::EventBody;
use protocol::model::Seq;

use super::SessionLog;

#[cfg(test)]
mod tests;

/// Where a recorded prompt stands with the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptState {
    /// Sent mid-turn, and the agent hasn't taken it yet. It takes it once it can, such as when a running tool finishes.
    Waiting,
    Taken,
    /// The agent never got it, because it was stopped first, or went away.
    Dropped,
}

impl SessionLog {
    /// Where the prompt recorded at `seq` stands, from what's loaded after it. The log records whether the agent took
    /// or dropped every prompt, and until it does, the prompt is waiting. One the agent dropped can still be taken by
    /// the agent after it.
    pub fn prompt_state(&self, seq: Seq) -> PromptState {
        let mut dropped = false;
        for later in seq + 1..=self.last_seq {
            // Not loaded, so far behind the tail, where it was settled long ago.
            let Some(event) = self.events.get(&later) else {
                return if dropped { PromptState::Dropped } else { PromptState::Taken };
            };
            if matches!(event.body, EventBody::PromptTaken { prompt } if prompt == seq) {
                return PromptState::Taken;
            }
            dropped |= matches!(event.body, EventBody::PromptDropped { prompt } if prompt == seq);
        }
        if dropped { PromptState::Dropped } else { PromptState::Waiting }
    }
}
