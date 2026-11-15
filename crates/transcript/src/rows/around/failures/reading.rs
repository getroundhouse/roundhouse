//! Which of the transcript's rows host a card of failures in a row, are said in one, or have nothing to do with one,
//! as the hub's word on each failure says, and which rows a failure arriving draws again.

use std::ops::Range;

use client_core::session_log::{Row, SessionLog};
use protocol::event::{EventBody, TurnOutcome};
use protocol::model::Seq;

use super::super::{NEAR, nearby};
use super::tries::{Tries, start_of};
use crate::folds::Folds;

/// What a row has to do with the agent's failures in a row.
pub(in crate::rows) enum Failures {
    /// Nothing: it's drawn as it is.
    Unrelated,
    /// It's said in a later failure's card, so it draws nothing: an earlier failure of the row, the line of a turn the
    /// agent started by itself that was refused before it did anything, or what came to nothing between two failures.
    InALaterCard,
    /// It's the card, the newest of its row.
    Card(Tries),
}

impl Failures {
    /// For the event at `seq`, as the log around it says.
    pub(in crate::rows) fn read(log: &SessionLog, folds: &Folds, seq: Seq, body: &EventBody) -> Self {
        if let EventBody::RequestFailed { subagent: None, again, .. } = body {
            if repeated_later(log, seq) {
                return Failures::InALaterCard;
            }
            return Failures::Card(Tries::read(log, seq, *again, folds));
        }
        let came_to_nothing =
            matches!(body, EventBody::TurnEnded { outcome: TurnOutcome::Success, request_failed: false, .. });
        let own_turn = matches!(body, EventBody::OwnTurn(_));
        let in_a_card =
            (own_turn && refused_first(log, seq)) || ((own_turn || came_to_nothing) && inside_a_row(log, seq));
        if in_a_card { Failures::InALaterCard } else { Failures::Unrelated }
    }

    pub(in crate::rows) fn in_a_later_card(&self) -> bool {
        matches!(self, Failures::InALaterCard)
    }

    /// The tries it says, if it's a card.
    pub(in crate::rows) fn card(&self) -> Option<&Tries> {
        if let Failures::Card(tries) = self { Some(tries) } else { None }
    }
}

/// Whether a later failure of the agent's own, within [`NEAR`] rows, repeats the one at `seq`, so it's said in that
/// one's card.
fn repeated_later(log: &SessionLog, seq: Seq) -> bool {
    nearby::events_after(log, seq, NEAR).any(|event| {
        matches!(&event.body, EventBody::RequestFailed { subagent: None, again: Some(again), .. }
            if again.previous == seq)
    })
}

/// Whether the turn the agent started by itself at `seq` was refused before it did anything: the next failure of the
/// agent's own reads where its try started from it, so its line is in that failure's card.
fn refused_first(log: &SessionLog, seq: Seq) -> bool {
    let next = nearby::events_after(log, seq, NEAR).find(|event| is_the_agents_own_failure(&event.body));
    next.and_then(|next| start_of(log, next.seq)).is_some_and(|start| start.from.seq == seq)
}

fn is_the_agents_own_failure(body: &EventBody) -> bool {
    matches!(body, EventBody::RequestFailed { subagent: None, .. })
}

/// Whether the event at `seq` is between two failures of the agent's own in a row, as the next failure after it says,
/// which the hub says only when nothing between them ended the row, so it came to nothing.
fn inside_a_row(log: &SessionLog, seq: Seq) -> bool {
    let next = nearby::events_after(log, seq, NEAR).find(|event| is_the_agents_own_failure(&event.body));
    next.is_some_and(
        |next| matches!(&next.body, EventBody::RequestFailed { again: Some(again), .. } if again.previous < seq),
    )
}

/// The rows to draw again when those in `changed` did, from as far as [`NEAR`] rows before them when they hold a
/// failure of the agent's own, which hosts the card that says what's before it: the failures it repeats, the line of
/// the turn it refused and what came to nothing between. A page that loads puts every row out of date already.
pub(crate) fn redrawn_with(log: &SessionLog, changed: Range<usize>) -> Range<usize> {
    let loaded = changed.start..changed.end.min(log.row_count());
    let fails =
        loaded.into_iter().any(|ix| matches!(log.row(ix), Row::Event(event) if is_the_agents_own_failure(&event.body)));
    if !fails {
        return changed;
    }
    changed.start.saturating_sub(NEAR)..changed.end
}
