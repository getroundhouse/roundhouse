//! A compaction as one line, for marking it in a long chat.

use serde::{Deserialize, Serialize};

use super::{Place, Seq};
use crate::event::{CompactedBy, CompactionOutcome, EventBody, SessionEvent};

/// A compaction the agent made of its own conversation, as one line: where it is in the log and among the agent's own
/// events, who started it, and how many tokens the context went from and to. Only one that compacted has a line, since
/// one that failed changed nothing, and only the agent's own, since a subagent's isn't in the chat.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct CompactionLine {
    pub seq: Seq,
    /// 0 from a hub that doesn't record places.
    #[serde(default)]
    pub place: Place,
    #[serde(default)]
    pub by: CompactedBy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_tokens: Option<u64>,
}

impl CompactionLine {
    /// The line for `event`, if it's a compaction of the agent's own conversation that compacted, as the hub lists it.
    pub fn of(event: &SessionEvent) -> Option<Self> {
        if event.subagent.is_some() {
            return None;
        }
        let EventBody::AgentCompacted { outcome, .. } = &event.body else { return None };
        let CompactionOutcome::Compacted { by, before_tokens, after_tokens, .. } = outcome else { return None };
        Some(Self {
            seq: event.seq,
            place: event.place,
            by: *by,
            before_tokens: *before_tokens,
            after_tokens: *after_tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(subagent: Option<&str>, body: EventBody) -> SessionEvent {
        SessionEvent {
            session_id: "s".into(),
            seq: 9,
            ts_ms: 0,
            run: None,
            subagent: subagent.map(Into::into),
            place: Place::new(7),
            body,
            raw: None,
        }
    }

    fn compacted(by: CompactedBy) -> EventBody {
        let outcome = CompactionOutcome::Compacted {
            by,
            before_tokens: Some(226_869),
            after_tokens: Some(9_009),
            took_ms: Some(51_558),
            summary: Some("The summary".to_string()),
        };
        EventBody::AgentCompacted { asked: None, keeping: None, outcome }
    }

    #[test]
    fn a_compaction_has_a_line_at_its_place() {
        let line = CompactionLine::of(&event(None, compacted(CompactedBy::Agent)));

        let at = CompactionLine {
            seq: 9,
            place: Place::new(7),
            by: CompactedBy::Agent,
            before_tokens: Some(226_869),
            after_tokens: Some(9_009),
        };
        assert_eq!(line, Some(at));
    }

    #[test]
    fn one_that_failed_a_subagents_and_any_other_event_have_none() {
        let failed = CompactionOutcome::Failed { error: Some("Not enough messages to compact.".to_string()) };
        let failed = EventBody::AgentCompacted { asked: Some(3), keeping: None, outcome: failed };

        assert_eq!(CompactionLine::of(&event(None, failed)), None);
        assert_eq!(CompactionLine::of(&event(Some("toolu-1"), compacted(CompactedBy::Agent))), None);
        assert_eq!(CompactionLine::of(&event(None, EventBody::AgentCompacting { written_chars: None })), None);
    }

    #[test]
    fn a_line_goes_to_a_client_with_the_fields_it_reads() {
        let line = CompactionLine::of(&event(None, compacted(CompactedBy::Asked))).unwrap();

        assert_eq!(
            serde_json::to_value(line).unwrap(),
            serde_json::json!({ "seq": 9, "place": 7, "by": "asked", "before_tokens": 226_869, "after_tokens": 9_009 })
        );
    }

    #[test]
    fn a_line_with_only_its_seq_reads_with_nothing_else_known() {
        let line: CompactionLine = serde_json::from_value(serde_json::json!({ "seq": 9 })).unwrap();

        assert_eq!(line.place, Place::default());
        assert_eq!(line.by, CompactedBy::Unknown);
        assert_eq!((line.before_tokens, line.after_tokens), (None, None));
    }
}
