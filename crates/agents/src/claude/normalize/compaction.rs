//! The CLI compacting its conversation: `compacting` statuses while it writes the summary, then a status with how it
//! went, then `compact_boundary` with what it freed, then the summary itself, for the model, on a line of its own. A
//! typed `/compact` also gets a replayed line saying `Compacted`.

use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
use serde_json::Value;
use tracing::warn;

use super::said::{self, Stream, text_of};
use super::{Parsed, other, str_of, string_of};

/// Before what the summary says, and after it, the CLI writes to the model rather than the person.
const SUMMARY_STARTS: &str = "\nSummary:\n";
const SUMMARY_ENDS: &str = "\n\nIf you need specific details from before compaction";

/// Remembers what a compaction's boundary said until the summary that follows it, so the two are one event.
#[derive(Debug, Default)]
pub(super) struct Compactions {
    ended: Option<Ended>,
}

/// What a boundary says of the compaction it ends.
#[derive(Debug)]
struct Ended {
    by: CompactedBy,
    before_tokens: Option<u64>,
    after_tokens: Option<u64>,
    took_ms: Option<u64>,
}

impl Compactions {
    /// What a line about the agent compacting says, or `None` for any other line, and for a subagent's. The boundary
    /// says nothing yet: the summary after it does, for both. In every compaction seen, the summary came on the very
    /// next line, so only the agent's next line can be it.
    pub(super) fn read(&mut self, value: &Value) -> Option<Parsed> {
        if string_of(value, "parent_tool_use_id").is_some() {
            return None;
        }
        if let Some(ended) = self.ended.take() {
            if let Some(text) = summary_line(value) {
                return Some(Parsed::Event(ended.with(summary_in(&text))));
            }
            // Its raw line is kept, so the compaction can still be read from it.
            warn!("a compaction's boundary wasn't followed by its summary, so it isn't recorded as one");
        }
        match str_of(value, "type")? {
            "system" => self.system(value),
            "user" => replayed(value),
            _ => None,
        }
    }

    fn system(&mut self, value: &Value) -> Option<Parsed> {
        match str_of(value, "subtype")? {
            // One that also says the mode is a change of mode.
            "status" if value.get("permissionMode").is_none() => status(value),
            "compact_boundary" => {
                self.ended = Some(Ended::read(value));
                Some(other("system/compact_boundary"))
            }
            _ => None,
        }
    }
}

/// The text of a line written for the model, which the summary is when it follows a boundary.
fn summary_line(value: &Value) -> Option<String> {
    let for_the_model = str_of(value, "type") == Some("user") && flag(value, "isSynthetic");
    for_the_model.then(|| text_of(value.get("message").and_then(|message| message.get("content"))))
}

/// What the CLI says to a typed `/compact`, which the compaction's own event says better.
fn replayed(value: &Value) -> Option<Parsed> {
    let (stream, said) = said::printed(value)?;
    (stream == Stream::Out && said == "Compacted").then(|| other("user/compacted"))
}

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool) == Some(true)
}

impl Ended {
    fn read(value: &Value) -> Self {
        let metadata = value.get("compact_metadata").unwrap_or(&Value::Null);
        let number = |key| metadata.get(key).and_then(Value::as_u64);
        let by = match str_of(metadata, "trigger") {
            Some("manual") => CompactedBy::Asked,
            Some("auto") => CompactedBy::Agent,
            _ => CompactedBy::Unknown,
        };
        Self {
            by,
            before_tokens: number("pre_tokens"),
            after_tokens: number("post_tokens"),
            took_ms: number("duration_ms"),
        }
    }

    /// The compaction, carrying on from `summary`, or from nothing it could tell if that's empty.
    fn with(self, summary: &str) -> EventBody {
        let outcome = CompactionOutcome::Compacted {
            by: self.by,
            before_tokens: self.before_tokens,
            after_tokens: self.after_tokens,
            took_ms: self.took_ms,
            summary: (!summary.is_empty()).then(|| summary.to_string()),
        };
        EventBody::AgentCompacted { asked: None, keeping: None, outcome }
    }
}

/// A status about compacting: that it's under way, with how much of the summary is written when the CLI says, or
/// that it failed. One that says it went well says nothing the boundary after it doesn't.
fn status(value: &Value) -> Option<Parsed> {
    if str_of(value, "status") == Some("compacting") {
        let written_chars = value.get("compact_output_chars").and_then(Value::as_u64);
        return Some(Parsed::Event(EventBody::AgentCompacting { written_chars }));
    }
    if str_of(value, "compact_result") != Some("failed") {
        return None;
    }
    let outcome = CompactionOutcome::Failed { error: str_of(value, "compact_error").map(str::to_string) };
    Some(Parsed::Event(EventBody::AgentCompacted { asked: None, keeping: None, outcome }))
}

/// What the summary says, without what's around it for the model. All of it, if the CLI wrote it some other way.
fn summary_in(text: &str) -> &str {
    let from = text.find(SUMMARY_STARTS).map_or(text, |at| &text[at + SUMMARY_STARTS.len()..]);
    from.find(SUMMARY_ENDS).map_or(from, |at| &from[..at]).trim()
}

#[cfg(test)]
mod tests {
    use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
    use serde_json::json;

    use super::super::{Normalizer, Parsed};

    fn parsed(normalizer: &mut Normalizer, line: &serde_json::Value) -> Parsed {
        normalizer.line(&line.to_string()).parsed
    }

    #[test]
    fn a_compaction_is_one_event_from_its_boundary_and_the_summary_after_it() {
        let mut normalizer = Normalizer::default();
        let boundary = json!({"type":"system","subtype":"compact_boundary","compact_metadata":{
            "trigger":"manual","pre_tokens":226_869,"post_tokens":9_009,"duration_ms":51_558}});
        let summary = json!({"type":"user","isSynthetic":true,"message":{"role":"user","content":
            "This session is being continued from a previous conversation.\n\n<system-reminder>\nx\n</system-reminder>\n\
             Summary:\n1. What was asked\n\nIf you need specific details from before compaction, read the transcript.\n\
             Continue the conversation."}});

        let first = parsed(&mut normalizer, &boundary);
        let second = parsed(&mut normalizer, &summary);

        assert!(matches!(first, Parsed::Event(EventBody::AgentOther { .. })), "{first:?}");
        let Parsed::Event(EventBody::AgentCompacted {
            outcome: CompactionOutcome::Compacted { by, before_tokens, after_tokens, took_ms, summary },
            ..
        }) = second
        else {
            panic!("{second:?}")
        };
        assert_eq!(
            (by, before_tokens, after_tokens, took_ms),
            (CompactedBy::Asked, Some(226_869), Some(9_009), Some(51_558))
        );
        assert_eq!(summary.as_deref(), Some("1. What was asked"), "without what's written for the model");
    }

    #[test]
    fn only_the_agents_next_line_after_a_boundary_can_be_its_summary() {
        let boundary = json!({"type":"system","subtype":"compact_boundary","compact_metadata":{"trigger":"auto"}});
        let summary = |parent: Option<&str>| {
            json!({"type":"user","isSynthetic":true,"parent_tool_use_id":parent,
                "message":{"role":"user","content":"Summary:\n1. What was asked"}})
        };
        let between = json!({"type":"rate_limit_event"});
        let compacted = |parsed: &Parsed| matches!(parsed, Parsed::Event(EventBody::AgentCompacted { .. }));

        let mut late = Normalizer::default();
        parsed(&mut late, &boundary);
        parsed(&mut late, &between);
        let mut a_subagents = Normalizer::default();
        parsed(&mut a_subagents, &boundary);
        let subagents = parsed(&mut a_subagents, &summary(Some("toolu-1")));

        assert!(!compacted(&parsed(&mut late, &summary(None))), "a line came between");
        assert!(!compacted(&subagents), "a subagent's line is its own");
        assert!(compacted(&parsed(&mut a_subagents, &summary(None))), "and the agent's next line is still the summary");
    }

    #[test]
    fn a_summary_with_nothing_in_it_is_none() {
        let mut normalizer = Normalizer::default();
        parsed(&mut normalizer, &json!({"type":"system","subtype":"compact_boundary"}));
        let empty = json!({"type":"user","isSynthetic":true,"message":{"role":"user","content":[]}});

        let summary = parsed(&mut normalizer, &empty);

        let none = |outcome: &CompactionOutcome| matches!(outcome, CompactionOutcome::Compacted { summary: None, .. });
        assert!(
            matches!(&summary, Parsed::Event(EventBody::AgentCompacted { outcome, .. }) if none(outcome)),
            "{summary:?}"
        );
    }

    #[test]
    fn a_line_for_the_model_with_no_boundary_before_it_isnt_a_compaction() {
        let mut normalizer = Normalizer::default();
        let shrunk =
            json!({"type":"user","isSynthetic":true,"message":{"role":"user","content":"The image was resized."}});

        let Parsed::Event(EventBody::AgentOther { kind, .. }) = parsed(&mut normalizer, &shrunk) else { panic!() };

        assert_eq!(kind, "user/synthetic");
    }

    #[test]
    fn compacting_says_how_much_of_the_summary_is_written_and_a_failure_says_why() {
        let mut normalizer = Normalizer::default();
        let writing = json!({"type":"system","subtype":"status","status":"compacting","compact_output_chars":8_463});
        let typed = json!({"type":"system","subtype":"status","status":"compacting"});
        let failed = json!({"type":"system","subtype":"status","status":null,"compact_result":"failed",
            "compact_error":"Not enough messages to compact."});
        let went_well = json!({"type":"system","subtype":"status","status":null,"compact_result":"success"});

        assert_eq!(
            parsed(&mut normalizer, &writing),
            Parsed::Event(EventBody::AgentCompacting { written_chars: Some(8_463) })
        );
        assert_eq!(parsed(&mut normalizer, &typed), Parsed::Event(EventBody::AgentCompacting { written_chars: None }));
        let Parsed::Event(EventBody::AgentCompacted { outcome, .. }) = parsed(&mut normalizer, &failed) else {
            panic!()
        };
        assert_eq!(outcome, CompactionOutcome::Failed { error: Some("Not enough messages to compact.".to_string()) });
        assert!(
            matches!(parsed(&mut normalizer, &went_well), Parsed::Event(EventBody::AgentOther { .. })),
            "the boundary says it"
        );
    }

    #[test]
    fn only_the_clis_compacted_line_is_left_out_of_what_it_replays() {
        let mut normalizer = Normalizer::default();
        let replayed = |said: &str| {
            json!({"type":"user","isReplay":true,"message":{"role":"user",
                "content":format!("<local-command-stdout>{said}</local-command-stdout>")}})
        };

        let compacted = parsed(&mut normalizer, &replayed("Compacted "));
        let model = parsed(&mut normalizer, &replayed("Set model to `haiku`"));

        assert!(matches!(compacted, Parsed::Event(EventBody::AgentOther { .. })), "{compacted:?}");
        assert!(matches!(model, Parsed::Event(EventBody::Notice { .. })), "what any other command printed: {model:?}");
    }
}
