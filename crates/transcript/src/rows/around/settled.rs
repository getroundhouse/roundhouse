//! What became of a question the agent asked once it stopped waiting, from the record that closed it, which keeps what
//! was answered.

use std::collections::BTreeMap;

use client_core::session_log::SessionLog;
use protocol::event::{ApprovalOutcome, EventBody};
use protocol::model::{ApprovalId, Seq};

use super::{NEAR, nearby};

/// What became of a question that's no longer waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::rows) enum Settled {
    /// What was answered, by the question's text. A question left out of it wasn't answered.
    Answered(BTreeMap<String, String>),
    /// Skipped, or closed with nobody left to answer it, such as when the turn was stopped.
    NotAnswered,
    /// Allowed with no answers kept, as a hub from before it kept them records an answer, or closed by a record that
    /// isn't near or isn't loaded, so what was answered isn't known.
    Unknown,
}

impl Settled {
    /// From the record that closed the approval `id`, asked for at `seq`, among the events near after it. The agent
    /// waits on a question, so little else is recorded before it's closed.
    pub(super) fn read(log: &SessionLog, seq: Seq, id: &ApprovalId) -> Self {
        let closed = nearby::events_after(log, seq, NEAR).find_map(|event| {
            let EventBody::ApprovalResolved { approval_id, outcome, answers_by_text: answers, .. } = &event.body else {
                return None;
            };
            (approval_id == id).then_some((*outcome, answers.as_ref()))
        });
        match closed {
            Some((ApprovalOutcome::Allowed, Some(answers))) => Settled::Answered(answers.clone()),
            Some((ApprovalOutcome::Denied | ApprovalOutcome::Cancelled, _)) => Settled::NotAnswered,
            Some((ApprovalOutcome::Allowed, None) | (ApprovalOutcome::Unknown, _)) | None => Settled::Unknown,
        }
    }

    /// What was answered to `question`, by its text.
    pub(in crate::rows) fn answer_to(&self, question: &str) -> AnswerTo<'_> {
        match self {
            Settled::Answered(answers) => answers.get(question).map_or(AnswerTo::NotAnswered, |a| AnswerTo::Said(a)),
            Settled::NotAnswered => AnswerTo::NotAnswered,
            Settled::Unknown => AnswerTo::Unknown,
        }
    }
}

/// What was answered to one question.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::rows) enum AnswerTo<'a> {
    Said(&'a str),
    NotAnswered,
    /// What was answered isn't known, as [`Settled::Unknown`] says.
    Unknown,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use protocol::event::{ApprovalOutcome, EventBody};

    use super::{AnswerTo, Settled};
    use crate::fixtures::{closed, event, log};

    fn asked(id: &str) -> EventBody {
        crate::fixtures::asked(id, "AskUserQuestion")
    }

    /// What became of the question `a`, asked first, with another's record and then `closing` after it.
    fn settled(closing: EventBody) -> Settled {
        let log = log([event(1, asked("a")), event(2, closed("b", ApprovalOutcome::Denied, None)), event(3, closing)]);
        Settled::read(&log, 1, &"a".into())
    }

    #[test]
    fn a_question_answered_keeps_what_was_answered_to_each() {
        let answered = settled(closed("a", ApprovalOutcome::Allowed, Some(&[("Red or blue?", "Blue")])));

        assert_eq!(answered.answer_to("Red or blue?"), AnswerTo::Said("Blue"));
        assert_eq!(answered.answer_to("Left out?"), AnswerTo::NotAnswered, "one left out wasn't answered");
        let expected: BTreeMap<String, String> = [("Red or blue?".into(), "Blue".into())].into();
        assert_eq!(answered, Settled::Answered(expected));
    }

    #[test]
    fn a_question_skipped_or_cut_short_was_not_answered() {
        assert_eq!(settled(closed("a", ApprovalOutcome::Denied, None)), Settled::NotAnswered);
        assert_eq!(settled(closed("a", ApprovalOutcome::Cancelled, None)), Settled::NotAnswered);
        assert_eq!(
            settled(closed("a", ApprovalOutcome::Denied, None)).answer_to("Red or blue?"),
            AnswerTo::NotAnswered
        );
    }

    #[test]
    fn what_was_answered_is_not_known_without_a_record_of_it() {
        let before_answers_were_kept = settled(closed("a", ApprovalOutcome::Allowed, None));

        assert_eq!(before_answers_were_kept, Settled::Unknown);
        assert_eq!(before_answers_were_kept.answer_to("Red or blue?"), AnswerTo::Unknown);
        assert_eq!(settled(closed("a", ApprovalOutcome::Unknown, Some(&[("Red or blue?", "Blue")]))), Settled::Unknown);
        let never_closed = log([event(1, asked("a")), event(2, closed("b", ApprovalOutcome::Denied, None))]);
        assert_eq!(Settled::read(&never_closed, 1, &"a".into()), Settled::Unknown, "only its own record closes it");
    }
}
