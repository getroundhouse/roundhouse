//! What became of a question the agent asked once it stopped waiting, from the record that closed it, which keeps what
//! was answered.

use client_core::Answered;
use client_core::session_log::SessionLog;
use protocol::capability::approvals::Decision;
use protocol::capability::asking::Question;
use protocol::event::EventBody;
use protocol::model::{ApprovalId, Seq};

use super::{NEAR, nearby};

/// What became of a question that's no longer waiting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::rows) enum Settled {
    /// What was answered, as the record keeps it. A question it says nothing of wasn't answered.
    Answered(Answered),
    /// Skipped, or closed with nobody left to answer it, such as when the turn was stopped.
    NotAnswered,
    /// Decided in a way this build can't read, or closed by a record that isn't near or isn't loaded, so what was
    /// answered isn't known.
    Unknown,
}

impl Settled {
    /// From the record that closed the approval `id`, asked for at `seq`, among the events near after it. The agent
    /// waits on a question, so little else is recorded before it's closed.
    pub(super) fn read(log: &SessionLog, seq: Seq, id: &ApprovalId) -> Self {
        let closed = nearby::events_after(log, seq, NEAR).find_map(|event| {
            let EventBody::ApprovalResolved { approval_id, decision, .. } = &event.body else { return None };
            (approval_id == id).then_some((decision.as_deref(), &event.body))
        });
        let Some((decision, body)) = closed else { return Settled::Unknown };
        match decision {
            Some(Decision::Answer { .. }) => Answered::of(body).map_or(Settled::Unknown, Settled::Answered),
            Some(Decision::Deny { .. }) | None => Settled::NotAnswered,
            Some(
                Decision::Allow { .. }
                | Decision::ApprovePlan { .. }
                | Decision::KeepPlanning { .. }
                | Decision::Grant { .. }
                | Decision::Respond(_)
                | Decision::Unknown,
            ) => Settled::Unknown,
        }
    }

    /// What was answered to `question`, asked at `at` among its questions.
    pub(in crate::rows) fn answer_to(&self, at: usize, question: &Question) -> AnswerTo {
        match self {
            Settled::Answered(answered) => answered.to(at, question).map_or(AnswerTo::NotAnswered, AnswerTo::Said),
            Settled::NotAnswered => AnswerTo::NotAnswered,
            Settled::Unknown => AnswerTo::Unknown,
        }
    }
}

/// What was answered to one question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::rows) enum AnswerTo {
    Said(String),
    NotAnswered,
    /// What was answered isn't known, as [`Settled::Unknown`] says.
    Unknown,
}

#[cfg(test)]
mod tests {
    use client_core::Answered;
    use protocol::capability::approvals::Decision;
    use protocol::capability::asking::{Question, QuestionAnswer};
    use protocol::event::EventBody;
    use protocol::fixtures::{allow_once, cancelled, decided, deny, question as asked_as};

    use super::{AnswerTo, Seq, Settled};
    use crate::fixtures::{asked_a_question as asked, event, log};

    fn question(text: &str) -> Question {
        asked_as(text, &[], false)
    }

    /// What became of the question `a`, asked first, with another's record and then `closing` after it.
    fn settled(closing: EventBody) -> Settled {
        let log = log([
            event(Seq::new(1), asked("a")),
            event(Seq::new(2), decided("b", deny(None))),
            event(Seq::new(3), closing),
        ]);
        Settled::read(&log, Seq::new(1), &"a".into())
    }

    #[test]
    fn a_question_answered_keeps_what_was_answered_to_each() {
        let typed = |text: &str| QuestionAnswer { picked: Vec::new(), typed: Some(text.into()) };
        let answers = vec![typed("Blue"), QuestionAnswer::default()];

        let answered = settled(decided("a", Decision::Answer { answers: answers.clone() }));

        assert_eq!(answered.answer_to(0, &question("Red or blue?")), AnswerTo::Said("Blue".into()));
        assert_eq!(
            answered.answer_to(1, &question("Left out?")),
            AnswerTo::NotAnswered,
            "one left out wasn't answered"
        );
        assert_eq!(answered, Settled::Answered(Answered(answers)));
    }

    #[test]
    fn a_question_skipped_or_cut_short_was_not_answered() {
        let skipped = || decided("a", Decision::Answer { answers: vec![QuestionAnswer::default()] });
        assert_eq!(settled(cancelled("a")), Settled::NotAnswered);
        assert_eq!(settled(skipped()).answer_to(0, &question("Red or blue?")), AnswerTo::NotAnswered);
    }

    #[test]
    fn what_was_answered_is_not_known_without_a_record_of_it() {
        let unknown = settled(decided("a", Decision::Unknown));

        assert_eq!(unknown, Settled::Unknown);
        assert_eq!(unknown.answer_to(0, &question("Red or blue?")), AnswerTo::Unknown);
        assert_eq!(settled(decided("a", allow_once())), Settled::Unknown, "a decision that answers no question");
        let never_closed = log([event(Seq::new(1), asked("a")), event(Seq::new(2), decided("b", deny(None)))]);
        assert_eq!(
            Settled::read(&never_closed, Seq::new(1), &"a".into()),
            Settled::Unknown,
            "only its own record closes it"
        );
    }
}
