use super::super::{Row, SessionLog};
use protocol::event::{EventBody, SessionEvent};
use protocol::model::{Seq, SettledAt, Settlement};
use std::collections::BTreeMap;

/// Where a recorded prompt stands with the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptState {
    /// Sent mid-turn, and the agent hasn't taken it yet. It takes it once it can, such as when a running tool finishes.
    Waiting,
    Taken,
    /// The agent never got it, because it was stopped first, or went away.
    Dropped,
}

/// Where a recorded prompt is drawn in the chat. The agent reads a prompt sent mid-turn only once it can, so it goes
/// after what the agent did before then, such as the thought it was writing when the prompt was sent, rather than
/// where the hub recorded it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptAt {
    /// In its own row: settled where the log doesn't say, or older than a prompt that was settled, as every prompt is
    /// in a chat from before the hub recorded which the agent took.
    Own,
    /// In this row, the one of the event that last settled it: where the agent took it, or where it was dropped.
    Settled(usize),
    /// At the end of the chat, after what the agent is still writing, since the agent hasn't read it yet.
    End,
}

/// Where each prompt was last settled, by where it was recorded, as [`SettledAt`] says: from the hub's list of prompts,
/// and from every event the log has been given that settles one. It stays when those events are let go of, since each
/// is small. A log of a stretch of the chat, read for finding, is given a copy, so it puts each prompt where the
/// chat does.
#[derive(Clone, Debug, Default)]
pub struct Settled(pub(super) BTreeMap<Seq, SettledAt>);

impl Settled {
    pub(super) fn at(&self, prompt: Seq) -> Option<SettledAt> {
        self.0.get(&prompt).copied()
    }

    /// Takes `at` for the prompt recorded at `prompt`, unless it's known to have been settled later, since the last
    /// event to settle it is where it's drawn. The same event at another place, as from a hub that reads by place
    /// after one that couldn't, takes that place. Returns whether anything changed.
    pub(super) fn settle(&mut self, prompt: Seq, at: SettledAt) -> bool {
        if self.0.get(&prompt).is_some_and(|known| known.seq > at.seq || *known == at) {
            return false;
        }
        self.0.insert(prompt, at);
        true
    }

    /// The newest prompt known to have been settled. A compaction asked for is settled the same way and counts too,
    /// since the hub refuses one while a prompt is waiting.
    pub(super) fn newest(&self) -> Option<Seq> {
        self.0.keys().next_back().copied()
    }
}

impl SessionLog {
    /// Where the prompt recorded at `seq` stands. The log records whether the agent took or dropped every prompt, and
    /// until it does, the prompt is waiting. One the agent dropped can still be taken by the agent after it, so the last
    /// to settle it says, as the hub's list of prompts does without its event being loaded. From a hub too old to say
    /// which, it's found from what's loaded after the prompt.
    pub fn prompt_state(&self, seq: Seq) -> PromptState {
        match self.settled.at(seq).and_then(|at| at.how) {
            Some(Settlement::Taken) => return PromptState::Taken,
            Some(Settlement::Dropped) => return PromptState::Dropped,
            Some(Settlement::Unknown) | None => {}
        }
        let mut dropped = false;
        // What comes after it in this log, which holds every event about a prompt, whether it's the whole log or the
        // agent's own events. One it doesn't hold, such as one past its end, has nothing after it yet.
        let Some(row) = self.row_of(seq) else { return PromptState::Waiting };
        for later in row + 1..self.row_count() {
            // Not loaded, so far behind the tail, where it was settled long ago.
            let Row::Event(event) = self.row(later) else {
                return if dropped { PromptState::Dropped } else { PromptState::Taken };
            };
            if matches!(event.body, EventBody::PromptTaken { prompt } if prompt == seq) {
                return PromptState::Taken;
            }
            dropped |= matches!(event.body, EventBody::PromptDropped { prompt } if prompt == seq);
        }
        if dropped { PromptState::Dropped } else { PromptState::Waiting }
    }

    /// Where the prompt recorded at `seq` is drawn. It waits at the end while it's unsettled and newer than every
    /// prompt that's been settled. The agent takes prompts in the order it was sent them, so one older than a prompt it
    /// took was settled too, even where the log doesn't say, as in a chat from before the hub recorded which it took,
    /// where none is settled and none waits.
    pub fn prompt_at(&self, seq: Seq) -> PromptAt {
        match self.settled.at(seq) {
            Some(at) => {
                // Until the event that settled it arrives, as when the hub's list says first, it stays at the end.
                let row = self.row_at(at.seq, at.place);
                if row < self.row_count() { PromptAt::Settled(row) } else { PromptAt::End }
            }
            None if self.settled.newest().is_some_and(|newest| seq > newest) => PromptAt::End,
            None => PromptAt::Own,
        }
    }

    /// The row the event recorded at `seq` is drawn in, if this log has it there: a prompt's as
    /// [`SessionLog::prompt_at`] says, which at the end is the tail's, after the log's rows, and any other event's own.
    pub fn row_drawing(&self, seq: Seq) -> Option<usize> {
        if !self.outline.is_prompt(seq) {
            return self.row_of(seq);
        }
        match self.prompt_at(seq) {
            PromptAt::Own => self.row_of(seq),
            PromptAt::Settled(row) => Some(row),
            PromptAt::End => Some(self.row_count()),
        }
    }

    /// The prompt `event` settled, if it's loaded and drawn in `event`'s row, as [`SessionLog::prompt_at`] says.
    pub fn prompt_settled_in(&self, event: &SessionEvent) -> Option<&SessionEvent> {
        let (prompt, _) = SettledAt::of(event)?;
        let row = self.row_of(event.seq)?;
        if self.prompt_at(prompt) != PromptAt::Settled(row) {
            return None;
        }
        let prompt = self.event_of(prompt)?;
        // A compaction asked for is settled the same way, and its own row draws it.
        matches!(prompt.body, EventBody::UserPrompt { .. }).then_some(prompt)
    }

    /// The recorded prompts drawn at the end of the chat, oldest first, as [`SessionLog::prompt_at`] says. The prompts
    /// on their way come after them, as [`SessionLog::pending`] has them.
    pub fn waiting_prompts(&self) -> Vec<&SessionEvent> {
        let mut waiting: Vec<&SessionEvent> =
            self.at_the_end().into_iter().filter_map(|seq| self.event_of(seq)).collect();
        waiting.reverse();
        waiting
    }

    /// Whether something the person sent is still waiting for the agent: a prompt on its way to the hub, as
    /// [`SessionLog::pending`] has them, or one recorded and not yet taken, which [`SessionLog::prompt_at`] puts at the
    /// end of the chat, whether or not it's loaded.
    pub fn has_prompts_waiting(&self) -> bool {
        !self.pending.is_empty() || !self.at_the_end().is_empty()
    }

    /// Where each prompt was last settled, for a log of a stretch of this chat.
    pub fn settled_prompts(&self) -> Settled {
        self.settled.clone()
    }

    /// The same log, knowing where each prompt was settled from the log of the chat it's a stretch of.
    #[must_use]
    pub fn knowing(self, settled: Settled) -> Self {
        Self { settled, ..self }
    }

    /// Where each prompt at the end was recorded, newest first.
    pub(in super::super) fn at_the_end(&self) -> Vec<Seq> {
        self.outline.newest_prompts().take_while(|seq| self.prompt_at(*seq) == PromptAt::End).collect()
    }

    /// The event recorded at `seq`, if it's loaded.
    pub(in super::super) fn event_of(&self, seq: Seq) -> Option<&SessionEvent> {
        self.events.get(&self.position_of(seq)?)
    }

    /// Notes that the prompt recorded at `prompt` was settled `at`, and tells a view of each row that moves.
    pub(in super::super) fn settle(&mut self, prompt: Seq, at: SettledAt) {
        let left = self.settled.at(prompt);
        if self.settled.settle(prompt, at) {
            self.prompt_moved(prompt, left);
        }
    }

    /// Tells a view what moved with an event, given where the prompts at the end were before it, `waited`. A prompt
    /// that leaves the end without being settled anew, as one the hub's list said was settled when the event that did
    /// arrives, moves to that event's row, and the outline puts each prompt at the end in the tail's row, which moves
    /// with every event.
    pub(in super::super) fn moved_from_the_end(&mut self, waited: &[Seq]) {
        let waiting = self.at_the_end();
        for prompt in waited.iter().filter(|prompt| !waiting.contains(prompt)) {
            self.prompt_moved(*prompt, None);
        }
        if !waited.is_empty() || !waiting.is_empty() {
            self.outline.rows_moved();
        }
    }

    /// The prompt recorded at `seq` may be drawn somewhere else now, as [`SessionLog::prompt_at`] says, so its own row,
    /// the row of where it was settled before, `left`, and of where it's settled now change, if they're loaded, and so
    /// does where the outline puts it. Only a live event moves one to or from the end, and it changes the tail anyway.
    pub(in super::super) fn prompt_moved(&mut self, seq: Seq, left: Option<SettledAt>) {
        let settled = [left, self.settled.at(seq)].into_iter().flatten().map(|at| self.row_at(at.seq, at.place));
        let rows: Vec<usize> = self.row_of(seq).into_iter().chain(settled).collect();
        for row in rows {
            if matches!(self.row(row), Row::Event(_)) {
                self.touched(row..row + 1);
            }
        }
        self.outline.rows_moved();
    }
}
