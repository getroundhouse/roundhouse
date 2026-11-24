//! Compactions, what asks for one and what says one is under way, in the form a hub stores and sends them.

use protocol::event::{CompactedBy, CompactionOutcome, EventBody};
use protocol::frame;
use protocol::model::Seq;
use protocol::tail::{Compacting, Tail};

/// A hub stores a compaction, and what asked for one, as these and finds them by these kinds, so both have to stay as
/// they are once one is recorded.
#[test]
fn a_compaction_and_what_asked_for_it_keep_their_form_and_the_kinds_they_re_stored_under() {
    let asked: &[u8] = br#"{"type":"compaction_asked","keeping":"The API design"}"#;
    let compacting: &[u8] = br#"{"type":"agent_compacting","written_chars":8463}"#;
    let compacted: &[u8] = br#"{"type":"agent_compacted","asked":7,"keeping":"The API design","outcome":{"result":"compacted","by":"asked","before_tokens":75284,"after_tokens":15050,"took_ms":31747,"summary":"1. What was asked"}}"#;
    let read = |body: &[u8]| -> EventBody { serde_json::from_slice(body).unwrap() };

    let outcome = CompactionOutcome::Compacted {
        by: CompactedBy::Asked,
        before_tokens: Some(75_284),
        after_tokens: Some(15_050),
        took_ms: Some(31_747),
        summary: Some("1. What was asked".into()),
    };
    assert_eq!(read(asked), EventBody::CompactionAsked { keeping: Some("The API design".into()) });
    assert_eq!(read(compacting), EventBody::AgentCompacting { written_chars: Some(8_463) });
    assert_eq!(
        read(compacted),
        EventBody::AgentCompacted { asked: Some(Seq::new(7)), keeping: Some("The API design".into()), outcome }
    );
    for (body, kind) in [(asked, "compaction_asked"), (compacting, "agent_compacting"), (compacted, "agent_compacted")]
    {
        assert_eq!(read(body).kind(), kind);
        assert_eq!(serde_json::to_vec(&read(body)).unwrap(), body, "{kind} keeps its form");
    }
}

/// A failure says only why, and a compaction, or an outcome, this build doesn't know still reads.
#[test]
fn a_compaction_that_failed_or_that_this_build_doesnt_know_still_reads() {
    let read = |body: &str| -> CompactionOutcome {
        let EventBody::AgentCompacted { outcome, .. } = serde_json::from_str(body).unwrap() else { panic!("{body}") };
        outcome
    };

    let failed = read(r#"{"type":"agent_compacted","outcome":{"result":"failed","error":"Not enough messages"}}"#);
    let newer = read(r#"{"type":"agent_compacted","outcome":{"result":"postponed"}}"#);
    let by_newer = read(r#"{"type":"agent_compacted","outcome":{"result":"compacted","by":"by_schedule"}}"#);
    let by_nobody = read(r#"{"type":"agent_compacted","outcome":{"result":"compacted"}}"#);

    assert_eq!(failed, CompactionOutcome::Failed { error: Some("Not enough messages".into()) });
    assert_eq!(
        (newer, read(r#"{"type":"agent_compacted"}"#)),
        (CompactionOutcome::Unknown, CompactionOutcome::Unknown)
    );
    for unknown in [by_newer, by_nobody] {
        assert!(matches!(unknown, CompactionOutcome::Compacted { by: CompactedBy::Unknown, .. }), "{unknown:?}");
    }
}

/// A turn's end from an agent that doesn't say it was only a compaction wasn't, and one that wasn't says nothing of it.
#[test]
fn a_turns_end_says_it_was_only_a_compaction_only_when_it_was() {
    let only: EventBody =
        serde_json::from_str(r#"{"type":"turn_ended","outcome":"success","result":"","only_compacted":true}"#).unwrap();
    let older: EventBody = serde_json::from_str(r#"{"type":"turn_ended","outcome":"success","result":null}"#).unwrap();

    assert!(matches!(only, EventBody::TurnEnded { only_compacted: true, .. }));
    let EventBody::TurnEnded { only_compacted, .. } = &older else { panic!() };
    assert!(!only_compacted);
    assert!(!serde_json::to_string(&older).unwrap().contains("only_compacted"));
}

#[test]
fn a_tail_that_doesnt_say_its_compacting_isnt_and_one_that_does_keeps_its_form() {
    let idle: Tail = frame::decode(br#"{"messages":[]}"#).unwrap();
    let compacting = br#"{"messages":[],"compacting":{"run":3,"written_chars":8463}}"#;

    let tail: Tail = frame::decode(compacting).unwrap();

    assert_eq!(idle.compacting, None);
    assert_eq!(tail.compacting.as_ref().and_then(|c| c.written_chars), Some(8_463));
    assert_eq!(frame::encode(&tail).unwrap(), [compacting.as_slice(), b"\n"].concat());
    assert_eq!(serde_json::from_str::<Compacting>("{}").unwrap(), Compacting::default(), "one that says nothing more");
}
