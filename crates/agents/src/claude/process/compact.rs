//! Which recorded request a compaction answers, what it said to keep, and whether its turn was only that. The CLI's
//! compaction doesn't name what asked for it, and only the adapter knows which recorded request each line it sent is.

use std::collections::HashMap;

use protocol::event::{CompactedBy, CompactionOutcome, EventBody, TurnOutcome};
use protocol::model::Seq;

/// A compaction someone asked for, as it's sent.
#[derive(Debug)]
pub(super) struct ToCompact {
    pub(super) keeping: Option<String>,
}

/// The compactions asked for, until the CLI takes or drops each, and then the one it took, until its turn ends.
#[derive(Debug, Default)]
pub(super) struct AskedToCompact {
    /// By the `uuid` each was sent with.
    sent: HashMap<String, ToCompact>,
    taken: Option<Taken>,
}

/// The compaction the CLI took, where it's recorded, and what its turn has held so far.
#[derive(Debug)]
struct Taken {
    asked: Seq,
    keeping: Option<String>,
    /// Its compaction has come.
    answered: bool,
    /// The agent has said something of its own since it took it.
    said: bool,
}

impl AskedToCompact {
    /// Notes the compaction sent as `uuid`.
    pub(super) fn sent(&mut self, uuid: &str, asked: ToCompact) {
        self.sent.insert(uuid.to_string(), asked);
    }

    /// The CLI took what was sent as `uuid`, which is recorded at `asked`.
    pub(super) fn taken(&mut self, uuid: &str, asked: Seq) {
        if let Some(ToCompact { keeping }) = self.sent.remove(uuid) {
            self.taken = Some(Taken { asked, keeping, answered: false, said: false });
        }
    }

    /// What was sent as `uuid` never reached the conversation.
    pub(super) fn dropped(&mut self, uuid: &str) {
        self.sent.remove(uuid);
    }

    /// Names the request a compaction answers, and marks the end of a turn that was only that compaction. The end of
    /// any turn forgets the request, so a later compaction the agent starts itself isn't put down to it.
    pub(super) fn answer(&mut self, body: &mut EventBody) {
        if let EventBody::TurnEnded { only_compacted, outcome, .. } = body {
            if let Some(taken) = self.taken.take() {
                // One that went wrong says so, and what's wrong, so its end is drawn.
                *only_compacted = taken.answered && !taken.said && *outcome == TurnOutcome::Success;
            }
            return;
        }
        let Some(taken) = &mut self.taken else { return };
        if let EventBody::Assistant { parent_tool_use_id: None, .. } = body {
            taken.said = true;
        }
        if let EventBody::AgentCompacted { asked, keeping, outcome } = body
            && !matches!(outcome, CompactionOutcome::Compacted { by: CompactedBy::Agent, .. })
            && !taken.answered
        {
            *asked = Some(taken.asked);
            keeping.clone_from(&taken.keeping);
            taken.answered = true;
            // A typed `/compact` reads as manual, but it says so either way.
            if let CompactionOutcome::Compacted { by, .. } = outcome {
                *by = CompactedBy::Asked;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{CompactedBy, CompactionOutcome, EventBody, TurnOutcome};

    use super::{AskedToCompact, ToCompact};

    fn compacted(by: CompactedBy) -> EventBody {
        let outcome =
            CompactionOutcome::Compacted { by, before_tokens: None, after_tokens: None, took_ms: None, summary: None };
        EventBody::AgentCompacted { asked: None, keeping: None, outcome }
    }

    fn turn_ended() -> EventBody {
        EventBody::TurnEnded {
            outcome: TurnOutcome::Success,
            result: None,
            errors: Vec::new(),
            duration_ms: None,
            cost_usd: None,
            num_turns: None,
            started_at_ms: None,
            models: Vec::new(),
            tokens: None,
            only_compacted: false,
        }
    }

    fn asked_and_taken(keeping: Option<&str>) -> AskedToCompact {
        let mut asked = AskedToCompact::default();
        asked.sent("u1", ToCompact { keeping: keeping.map(str::to_string) });
        asked.taken("u1", 7);
        asked
    }

    fn only_compacted(end: &EventBody) -> bool {
        matches!(end, EventBody::TurnEnded { only_compacted: true, .. })
    }

    #[test]
    fn a_compaction_names_the_request_the_cli_took_and_its_turn_was_only_that() {
        let mut asked = asked_and_taken(Some("Keep the API design"));
        let (mut body, mut end) = (compacted(CompactedBy::Unknown), turn_ended());

        asked.answer(&mut body);
        asked.answer(&mut end);

        let EventBody::AgentCompacted { asked: named, keeping, outcome } = body else { unreachable!() };
        assert_eq!((named, keeping.as_deref()), (Some(7), Some("Keep the API design")));
        assert!(matches!(outcome, CompactionOutcome::Compacted { by: CompactedBy::Asked, .. }));
        assert!(only_compacted(&end));
    }

    #[test]
    fn a_failure_names_the_request_too_and_a_turn_that_went_wrong_isnt_only_a_compaction() {
        let failed = || EventBody::AgentCompacted {
            asked: None,
            keeping: None,
            outcome: CompactionOutcome::Failed { error: None },
        };
        let (mut quietly, mut went_wrong) = (asked_and_taken(None), asked_and_taken(None));
        let (mut first, mut second) = (failed(), failed());
        let (mut ended, mut errored) = (turn_ended(), turn_ended());
        if let EventBody::TurnEnded { outcome, .. } = &mut errored {
            *outcome = TurnOutcome::Error;
        }

        for (asked, body, end) in [(&mut quietly, &mut first, &mut ended), (&mut went_wrong, &mut second, &mut errored)]
        {
            asked.answer(body);
            asked.answer(end);
        }

        assert!(matches!(first, EventBody::AgentCompacted { asked: Some(7), .. }));
        assert!(only_compacted(&ended), "its failure says it all");
        assert!(!only_compacted(&errored), "the turn's end says what went wrong");
    }

    #[test]
    fn a_turn_the_agent_said_something_in_or_that_never_compacted_wasnt_only_a_compaction() {
        let said = EventBody::Assistant {
            message_id: None,
            parent_tool_use_id: None,
            model: None,
            blocks: Vec::new(),
            first_index: None,
        };
        let mut talked = asked_and_taken(None);
        let mut end = turn_ended();
        talked.answer(&mut compacted(CompactedBy::Asked));
        talked.answer(&mut said.clone());
        talked.answer(&mut end);
        let mut never = asked_and_taken(None);
        let mut nothing = turn_ended();

        never.answer(&mut nothing);

        assert!(!only_compacted(&end), "the agent said something");
        assert!(!only_compacted(&nothing), "no compaction came");
    }

    #[test]
    fn one_the_agent_started_or_after_the_turn_ended_or_never_taken_names_no_request() {
        let mut asked = asked_and_taken(None);
        let mut by_itself = compacted(CompactedBy::Agent);
        asked.answer(&mut by_itself);
        asked.answer(&mut turn_ended());
        let mut later = compacted(CompactedBy::Unknown);
        asked.answer(&mut later);
        let mut dropped = AskedToCompact::default();
        dropped.sent("u2", ToCompact { keeping: None });
        dropped.dropped("u2");
        dropped.taken("u2", 4);
        let mut never = compacted(CompactedBy::Asked);

        dropped.answer(&mut never);

        for body in [by_itself, later, never] {
            assert!(matches!(body, EventBody::AgentCompacted { asked: None, .. }), "{body:?}");
        }
    }
}
