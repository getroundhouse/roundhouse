//! The parts of a tail: what one agent or subagent writes, and what one run does, which a worker hands a hub that
//! connects to it.

use std::collections::BTreeSet;

use super::Covering;
use super::stream::{Compacting, InBackground, Retrying, Tail, TailMessage};
use crate::model::{RunId, SubagentId};

impl Tail {
    /// What the agent itself is writing, or the subagent `subagent`. It holds no other's messages for the end of a
    /// turn to leave, so it doesn't say which subagents are in the background.
    #[must_use]
    pub fn of(&self, subagent: Option<&SubagentId>) -> Tail {
        let messages = self.messages.iter().filter(|message| message.subagent.as_ref() == subagent);
        // Only the agent compacts its conversation, says it's retrying, has a line said for it or has its replies read
        // out, so a subagent's tail never says any of those.
        let agents = subagent.is_none();
        let compacting = self.compacting.clone().filter(|_| agents);
        let retrying = self.retrying.filter(|_| agents);
        let covering = self.covering.clone().filter(|_| agents);
        let speaking = self.speaking.clone().filter(|_| agents);
        let messages = messages.cloned().collect();
        Tail { messages, in_background: Vec::new(), compacting, retrying, covering, speaking }
    }

    /// Every run it holds something of, oldest first.
    #[must_use]
    pub fn runs(&self) -> Vec<RunId> {
        let messages = self.messages.iter().map(|message| message.run);
        let in_background = self.in_background.iter().map(|background| background.run);
        let compacting = self.compacting.iter().map(|compacting| compacting.run);
        let retrying = self.retrying.iter().map(|retrying| retrying.run);
        let covering = self.covering.iter().map(|covering| covering.run);
        let said = compacting.chain(retrying).chain(covering);
        let runs: BTreeSet<RunId> = messages.chain(in_background).chain(said).flatten().collect();
        runs.into_iter().collect()
    }

    /// What `run` is writing, the subagents it runs in the background, its compaction, its retrying and the line said
    /// for it, which a worker sends a hub that connects to it.
    #[must_use]
    pub fn of_run(&self, run: RunId) -> Tail {
        let of = |by: Option<RunId>| by == Some(run);
        Tail {
            messages: self.messages.iter().filter(|message| of(message.run)).cloned().collect(),
            in_background: self.in_background.iter().filter(|background| of(background.run)).cloned().collect(),
            compacting: self.compacting.clone().filter(|compacting| of(compacting.run)),
            retrying: self.retrying.filter(|retrying| of(retrying.run)),
            covering: self.covering.clone().filter(|covering| of(covering.run)),
            // In no run.
            speaking: None,
        }
    }

    /// Takes what `run` is writing from `written`, its worker's tail of it, in place of what this holds of it, as a hub
    /// does from a worker it connects to again, since the deltas that wrote it went to the hub before, or to none.
    /// Everything in `written` is taken as `run`'s. A compaction only asked for so far is the hub's own, so it stays
    /// unless `written` says `run` is compacting.
    pub fn replace_run(&mut self, run: RunId, written: Tail) {
        let of = |by: Option<RunId>| by == Some(run);
        self.messages.retain(|message| !of(message.run));
        self.messages.extend(written.messages.into_iter().map(|message| TailMessage { run: Some(run), ..message }));
        self.in_background.retain(|background| !of(background.run));
        let in_background = written.in_background.into_iter();
        self.in_background.extend(in_background.map(|background| InBackground { run: Some(run), ..background }));
        self.compacting = match written.compacting {
            Some(compacting) => Some(Compacting { run: Some(run), ..compacting }),
            None => self.compacting.take().filter(|compacting| !of(compacting.run)),
        };
        self.retrying = match written.retrying {
            Some(retrying) => Some(Retrying { run: Some(run), ..retrying }),
            None => self.retrying.take().filter(|retrying| !of(retrying.run)),
        };
        self.covering = match written.covering {
            Some(covering) => Some(Box::new(Covering { run: Some(run), ..*covering })),
            None => self.covering.take().filter(|covering| !of(covering.run)),
        };
    }
}
