//! What started each of the tries a card says, the same failure in a row, as the events of each one's turn before it
//! say.

use client_core::session_log::SessionLog;
use protocol::capability::turns::OwnTurn;
use protocol::event::{Again, EndedTask, EventBody, SessionEvent};
use protocol::model::Seq;

use super::super::{NEAR, nearby};
use crate::folds::{Fold, Folds};
use crate::item_key::ItemKey;

/// How many of the newest tries a card shows before the rest fold.
pub(super) const SHOWN_TRIES: usize = 4;

/// The tries a card says, the same failure in a row.
pub(in crate::rows) struct Tries {
    /// How many, as the hub counted them, whether or not each is loaded.
    pub(in crate::rows) times: u32,
    /// What started each try that's loaded and near enough to its card, and whose start the log says, oldest first,
    /// up to the newest.
    pub(in crate::rows) each: Vec<Try>,
    /// What the fold of the earlier tries is known by: the oldest failure the card reaches, which stays its oldest as
    /// tries join, so the fold stays as the person left it.
    pub(in crate::rows) fold: ItemKey,
    /// Whether the earlier tries, past the newest [`SHOWN_TRIES`], are open.
    pub(in crate::rows) open: bool,
}

/// One try that failed, and when it started.
pub(in crate::rows) struct Try {
    pub(in crate::rows) started: Started,
    /// When it started, or for one partway through a turn, when the agent was last at work before it failed.
    pub(in crate::rows) at_ms: i64,
}

/// What started a try.
pub(in crate::rows) enum Started {
    /// The agent picked up by itself, as the tasks it heard had ended.
    Tasks(Vec<EndedTask>),
    /// The person's prompt.
    Prompt,
    /// Nothing: the agent was partway through its turn when it was refused.
    Partway,
    /// The agent picked up by itself, for a reason the chat says nothing more of.
    ByItself,
}

/// Where a try started: the event its start is read from, and what that says.
pub(super) struct Start<'a> {
    pub(super) from: &'a SessionEvent,
    pub(super) started: Started,
    pub(super) at_ms: i64,
}

impl Tries {
    /// The tries of the row the failure at `seq` ends, following what each says it repeats as far as [`repeated`] goes.
    pub(super) fn read(log: &SessionLog, seq: Seq, again: Option<Again>, folds: &Folds) -> Self {
        let mut each = Vec::new();
        let (mut oldest, mut at) = (seq, Some((seq, again)));
        while let Some((seq, again)) = at {
            if let Some(start) = start_of(log, seq) {
                each.push(Try { started: start.started, at_ms: start.at_ms });
            }
            oldest = seq;
            at = again.and_then(|again| repeated(log, seq, again.previous));
        }
        each.reverse();
        let fold = ItemKey::Event(oldest, 1);
        let open = folds.is_open(&fold, Fold::Earlier);
        Self { times: again.map_or(1, |again| again.times), each, fold, open }
    }

    /// Whether the card says what started each try. A single try after a prompt, whose bubble is just above, or partway
    /// through a turn, whose work is, needs no line, and a turn the agent started by itself has its line here.
    pub(in crate::rows) fn says_each(&self) -> bool {
        match self.each.as_slice() {
            [] => false,
            [only] if self.times == 1 => !matches!(only.started, Started::Prompt | Started::Partway),
            _ => true,
        }
    }

    /// How many of the tries it says fold, which is fewer than the hub counted when some aren't loaded.
    pub(in crate::rows) fn earlier(&self) -> usize {
        self.each.len().saturating_sub(SHOWN_TRIES)
    }

    /// The tries the card shows, oldest first: every one it says when they're open, and otherwise the newest few.
    pub(in crate::rows) fn shown(&self) -> &[Try] {
        if self.open {
            return &self.each;
        }
        &self.each[self.earlier()..]
    }
}

/// The failure at `previous`, which the one at `seq` says it repeats, and what it says it repeats in turn, if it's the
/// agent's own, loaded, and within [`NEAR`] rows before it, as far as a failure looks for a later one that repeats it.
/// One farther back is a card of its own. Each step goes back, so a chain that says otherwise ends.
fn repeated(log: &SessionLog, seq: Seq, previous: Seq) -> Option<(Seq, Option<Again>)> {
    let event = nearby::events_before(log, seq, NEAR).find(|event| event.seq == previous)?;
    let EventBody::RequestFailed { parent_tool_use_id: None, again, .. } = &event.body else { return None };
    Some((previous, *again))
}

/// Where the try that failed at `seq` started, as the events of its turn before it say, nearest first, back as far as
/// [`NEAR`] rows. Nothing when a row on the way isn't loaded, or none of them says, rather than a guess.
pub(super) fn start_of(log: &SessionLog, seq: Seq) -> Option<Start<'_>> {
    // The event after the one being read, nearest it, which is where a turn after another's end started.
    let mut after = nearby::event_at(log, seq);
    for event in nearby::loaded_before(log, seq).take(NEAR) {
        if let Some(started) = started_by(event) {
            let ended = matches!(event.body, EventBody::TurnEnded { .. });
            let at_ms = if ended { after.map_or(event.ts_ms, |after| after.ts_ms) } else { event.ts_ms };
            return Some(Start { from: event, started, at_ms });
        }
        after = Some(event);
    }
    None
}

/// What `event`, before a failure in its turn, says of what started the try, if anything: a turn the agent started by
/// itself, why; the agent's own words or calls, that it was partway through; a prompt; or the turn before's end, that
/// nothing in this one said.
fn started_by(event: &SessionEvent) -> Option<Started> {
    let body = &event.body;
    if let EventBody::OwnTurn(OwnTurn::TaskEnded { ended }) = body {
        return Some(Started::Tasks(ended.clone()));
    }
    if matches!(body, EventBody::OwnTurn(_) | EventBody::TurnEnded { .. }) {
        return Some(Started::ByItself);
    }
    if matches!(body, EventBody::UserPrompt { .. } | EventBody::PromptTaken { .. }) {
        return Some(Started::Prompt);
    }
    is_the_agents_own_work(body).then_some(Started::Partway)
}

/// The agent's own words or calls, or what came back for them.
fn is_the_agents_own_work(body: &EventBody) -> bool {
    matches!(
        body,
        EventBody::Assistant { parent_tool_use_id: None, .. } | EventBody::AgentUser { parent_tool_use_id: None, .. }
    )
}
