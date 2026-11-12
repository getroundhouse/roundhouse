//! What became of a question the agent asked once it stopped waiting, from the record that closed it, which keeps what
//! was answered.

use std::borrow::Cow;

use client_core::Answered;
use client_core::session_log::SessionLog;
use protocol::event::{ApprovalOutcome, EventBody, Question};
use protocol::model::{ApprovalId, Seq};

use super::{NEAR, nearby};

/// What became of a question that's no longer waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::rows) enum Settled {
    /// What was answered, as the record keeps it. A question it says nothing of wasn't answered.
    Answered(Answered),
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
            let EventBody::ApprovalResolved { approval_id, outcome, .. } = &event.body else { return None };
            (approval_id == id).then(|| (*outcome, Answered::of(&event.body)))
        });
        match closed {
            Some((ApprovalOutcome::Allowed, Some(answered))) => Settled::Answered(answered),
            Some((ApprovalOutcome::Denied | ApprovalOutcome::Cancelled, _)) => Settled::NotAnswered,
            Some((ApprovalOutcome::Allowed, None) | (ApprovalOutcome::Unknown, _)) | None => Settled::Unknown,
        }
    }

    /// What was answered to `question`, asked at `at` among its questions.
    pub(in crate::rows) fn answer_to(&self, at: usize, question: &Question) -> AnswerTo<'_> {
        match self {
            Settled::Answered(answered) => answered.to(at, question).map_or(AnswerTo::NotAnswered, AnswerTo::Said),
            Settled::NotAnswered => AnswerTo::NotAnswered,
            Settled::Unknown => AnswerTo::Unknown,
        }
    }
}

/// What was answered to one question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::rows) enum AnswerTo<'a> {
    Said(Cow<'a, str>),
    NotAnswered,
    /// What was answered isn't known, as [`Settled::Unknown`] says.
    Unknown,
}

#[cfg(test)]
mod tests {
    use client_core::Answered;
    use protocol::event::{ApprovalOutcome, EventBody, Question, QuestionAnswer};

    use super::{AnswerTo, Settled};
    use crate::fixtures::{closed, closed_by_position, event, log};

    fn asked(id: &str) -> EventBody {
        crate::fixtures::asked(id, "AskUserQuestion")
    }

    fn question(text: &str) -> Question {
        Question { text: text.into(), header: None, options: Vec::new(), multiple: false }
    }

    /// What became of the question `a`, asked first, with another's record and then `closing` after it.
    fn settled(closing: EventBody) -> Settled {
        let log = log([event(1, asked("a")), event(2, closed("b", ApprovalOutcome::Denied, None)), event(3, closing)]);
        Settled::read(&log, 1, &"a".into())
    }

    #[test]
    fn a_question_answered_keeps_what_was_answered_to_each() {
        let typed = |text: &str| QuestionAnswer { picked: Vec::new(), typed: Some(text.into()) };
        let answers = vec![typed("Blue"), QuestionAnswer::default()];

        let answered = settled(closed_by_position("a", answers.clone()));

        assert_eq!(answered.answer_to(0, &question("Red or blue?")), AnswerTo::Said("Blue".into()));
        assert_eq!(
            answered.answer_to(1, &question("Left out?")),
            AnswerTo::NotAnswered,
            "one left out wasn't answered"
        );
        assert_eq!(answered, Settled::Answered(Answered::ByPosition(answers)));
    }

    #[test]
    fn a_question_answered_by_text_keeps_what_was_answered_to_each() {
        let answered = settled(closed("a", ApprovalOutcome::Allowed, Some(&[("Red or blue?", "Blue")])));

        assert_eq!(answered.answer_to(0, &question("Red or blue?")), AnswerTo::Said("Blue".into()));
        assert_eq!(answered.answer_to(1, &question("Left out?")), AnswerTo::NotAnswered);
    }

    #[test]
    fn a_question_skipped_or_cut_short_was_not_answered() {
        assert_eq!(settled(closed("a", ApprovalOutcome::Denied, None)), Settled::NotAnswered);
        assert_eq!(settled(closed("a", ApprovalOutcome::Cancelled, None)), Settled::NotAnswered);
        assert_eq!(
            settled(closed("a", ApprovalOutcome::Denied, None)).answer_to(0, &question("Red or blue?")),
            AnswerTo::NotAnswered
        );
    }

    #[test]
    fn what_was_answered_is_not_known_without_a_record_of_it() {
        let before_answers_were_kept = settled(closed("a", ApprovalOutcome::Allowed, None));

        assert_eq!(before_answers_were_kept, Settled::Unknown);
        assert_eq!(before_answers_were_kept.answer_to(0, &question("Red or blue?")), AnswerTo::Unknown);
        assert_eq!(settled(closed("a", ApprovalOutcome::Unknown, Some(&[("Red or blue?", "Blue")]))), Settled::Unknown);
        let never_closed = log([event(1, asked("a")), event(2, closed("b", ApprovalOutcome::Denied, None))]);
        assert_eq!(Settled::read(&never_closed, 1, &"a".into()), Settled::Unknown, "only its own record closes it");
    }
}
