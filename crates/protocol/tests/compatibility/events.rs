//! Events a hub kept in its log in the form an older hub wrote them, which every later hub and client has to read.

use protocol::capability::turns::OwnTurn;
use protocol::event::{Again, EventBody, FailureKind, RequestFailure, TaskChange, TaskKind, TaskOutcome};
use protocol::model::{Seq, WaitEnd, Watching};

/// A hub stores events in the form it sends, so an agent's start recorded before hubs said which build it ran on has to
/// read as saying nothing of it, which a chat takes as not having been updated.
#[test]
fn an_agent_s_start_from_before_builds_were_recorded_says_nothing_of_them() {
    let before: EventBody = serde_json::from_str(r#"{"type":"agent_started","pid":7,"resumed":true}"#).unwrap();
    let EventBody::AgentStarted { pid: 7, resumed: true, build: None, previous_build: None, capabilities: None } =
        before
    else {
        panic!("{before:?}");
    };
    let updated = EventBody::AgentStarted {
        pid: 7,
        resumed: true,
        build: Some("1.0.0".into()),
        previous_build: Some("0.9.0".into()),
        capabilities: None,
    };
    assert_eq!(
        serde_json::to_string(&updated).unwrap(),
        r#"{"type":"agent_started","pid":7,"resumed":true,"build":"1.0.0","previous_build":"0.9.0"}"#
    );
    let mut claims = protocol::capability::Capabilities::default();
    claims.prompts.steer = Some(protocol::capability::Claimed {});
    let claimed = EventBody::AgentStarted {
        pid: 7,
        resumed: false,
        build: None,
        previous_build: None,
        capabilities: Some(Box::new(claims)),
    };
    let stored = r#"{"type":"agent_started","pid":7,"resumed":false,"capabilities":{"conversation":{},"prompts":{"steer":{}},"output":{},"calls":{},"approvals":{},"asking":{},"settings":{},"turns":{},"context":{},"account":{},"background":{},"setup":{}}}"#;
    assert_eq!(serde_json::to_string(&claimed).unwrap(), stored, "a run's claims are kept where it started");
    assert_eq!(serde_json::from_str::<EventBody>(stored).unwrap(), claimed);
}

/// A hub stores what an agent says its subagent's conversation holds as this and under this kind, so both have to stay
/// as they are once one is recorded. A subagent's end recorded before ends counted says nothing of it, and one that
/// counts none is stored as those were.
#[test]
fn what_a_subagents_conversation_holds_keeps_the_form_and_the_kind_its_stored_under() {
    let stored = r#"{"type":"subagent_context","call":"toolu_1","tokens":37386}"#;
    let before = r#"{"type":"subagent_ended","call":"toolu_1","outcome":"finished","report":"Done."}"#;

    let holds: EventBody = serde_json::from_str(stored).unwrap();
    let ended: EventBody = serde_json::from_str(before).unwrap();

    assert_eq!(holds, EventBody::SubagentContext { subagent: "toolu_1".into(), tokens: 37_386 });
    assert_eq!(holds.kind(), "subagent_context");
    assert_eq!(serde_json::to_string(&holds).unwrap(), stored);
    let EventBody::SubagentEnded { context_tokens: None, .. } = &ended else { panic!("{ended:?}") };
    assert_eq!(serde_json::to_string(&ended).unwrap(), before);
    let counted = before.replace('}', r#","context_tokens":41020}"#);
    let EventBody::SubagentEnded { context_tokens: Some(41_020), .. } = serde_json::from_str(&counted).unwrap() else {
        panic!("{counted}");
    };
}

/// A hub stores what the agent said of one of its background tasks in the form it sends, under this kind. A change a
/// later agent names reads as one this build doesn't know, so the line is still kept.
#[test]
fn a_change_to_a_task_keeps_the_form_and_the_kind_its_stored_under() {
    let stored = r#"{"type":"task_changed","task":"t1","change":"started"}"#;
    let later = r#"{"type":"task_changed","task":"t1","change":"something_newer"}"#;

    let changed: EventBody = serde_json::from_str(stored).unwrap();

    assert_eq!(changed, EventBody::TaskChanged { task: "t1".into(), change: TaskChange::Started });
    assert_eq!(changed.kind(), "task_changed");
    assert_eq!(serde_json::to_string(&changed).unwrap(), stored);
    for (change, said) in
        [(TaskChange::Progressed, "progressed"), (TaskChange::Updated, "updated"), (TaskChange::Ended, "ended")]
    {
        assert_eq!(serde_json::to_value(change).unwrap(), said);
    }
    let EventBody::TaskChanged { change: TaskChange::Unknown, .. } = serde_json::from_str(later).unwrap() else {
        panic!("{later}")
    };
}

/// A hub stores a failed request in the form it sends, under this kind. A kind of failure a later agent names reads as
/// one nobody named, so the card is still drawn.
#[test]
fn a_failed_request_keeps_the_form_and_the_kind_its_stored_under() {
    let stored = r#"{"type":"request_failed","parent_tool_use_id":"toolu_1","failure":{"message":"You've reached your limit.","kind":"limit","resets_at_ms":1700000000000}}"#;
    let later = r#"{"type":"request_failed","failure":{"message":"Try again.","kind":"something_newer"}}"#;
    let bare = r#"{"type":"request_failed","failure":{"message":"Try again."}}"#;

    let failed: EventBody = serde_json::from_str(stored).unwrap();

    let failure = RequestFailure {
        message: "You've reached your limit.".into(),
        kind: FailureKind::Limit,
        resets_at_ms: Some(1_700_000_000_000),
    };
    assert_eq!(failed, EventBody::RequestFailed { subagent: Some("toolu_1".into()), failure, again: None });
    assert_eq!(failed.kind(), "request_failed");
    assert_eq!(serde_json::to_string(&failed).unwrap(), stored);
    for line in [later, bare] {
        let EventBody::RequestFailed { subagent: None, failure, .. } = serde_json::from_str(line).unwrap() else {
            panic!("{line}")
        };
        assert_eq!((failure.kind, failure.resets_at_ms), (FailureKind::Unknown, None), "{line}");
    }
}

/// The hub stores what a failed request repeats beside it, and one stored without, as the agent sends it and as every
/// failure was recorded before the hub said, repeats nothing.
#[test]
fn a_failed_request_that_repeats_the_one_before_keeps_the_form_its_stored_in() {
    let stored = r#"{"type":"request_failed","failure":{"message":"You've reached your limit.","kind":"limit"},"again":{"previous":12,"times":3}}"#;

    let failed: EventBody = serde_json::from_str(stored).unwrap();

    let failure =
        RequestFailure { message: "You've reached your limit.".into(), kind: FailureKind::Limit, resets_at_ms: None };
    let again = Some(Again { previous: Seq::new(12), times: 3 });
    assert_eq!(failed, EventBody::RequestFailed { subagent: None, failure, again });
    assert_eq!(serde_json::to_string(&failed).unwrap(), stored);
}

/// A notice from before notices could be a subagent's is the agent's own, and one that is says nothing of it, so it's
/// stored as those were.
#[test]
fn a_notice_from_before_notices_were_a_subagents_is_the_agents_own() {
    let before = r#"{"type":"notice","level":"info","message":"Set model to `opus`"}"#;

    let notice: EventBody = serde_json::from_str(before).unwrap();

    let EventBody::Notice { subagent: None, .. } = &notice else { panic!("{notice:?}") };
    assert_eq!(serde_json::to_string(&notice).unwrap(), before);
}

/// A turn's end from before an agent said it failed on a request didn't, so it says why itself, and one that didn't
/// says nothing of it.
#[test]
fn a_turns_end_says_it_failed_on_a_request_only_when_it_did() {
    let older = r#"{"type":"turn_ended","outcome":"error","result":null}"#;

    let ended: EventBody = serde_json::from_str(older).unwrap();
    let failed: EventBody = serde_json::from_str(&older.replace('}', r#","request_failed":true}"#)).unwrap();

    let EventBody::TurnEnded { request_failed: false, .. } = &ended else { panic!("{ended:?}") };
    assert!(!serde_json::to_string(&ended).unwrap().contains("request_failed"));
    assert!(matches!(failed, EventBody::TurnEnded { request_failed: true, .. }), "{failed:?}");
}

/// A hub stores a turn the agent started by itself in the form it sends, under this kind, with what it kept of a wait
/// that ended. A reason a later agent gives, and what a later hub's wait watched, read as ones this build doesn't know.
#[test]
fn a_turn_the_agent_started_by_itself_keeps_the_form_and_the_kind_its_stored_under() {
    let stored = r#"{"type":"own_turn","why":"task_ended","ended":[{"task":{"id":"b1","description":"Run the tests","kind":"command"},"outcome":"failed"},{"task":{"id":"k2","description":null,"kind":"wait","call":"toolu_2"},"outcome":"finished","wait":{"id":"w","watching":{"on":"branch","number":12,"until":"all_checks"},"ended":{"at_ms":9,"how":"checks_passed"}}}]}"#;
    let later = r#"{"type":"own_turn","why":"woke_from_a_dream"}"#;

    let own: EventBody = serde_json::from_str(stored).unwrap();

    let EventBody::OwnTurn(OwnTurn::TaskEnded { ended }) = &own else { panic!("{own:?}") };
    let how = |kind, outcome| (kind, outcome);
    let read: Vec<_> = ended.iter().map(|ended| how(ended.task.kind, ended.outcome)).collect();
    assert_eq!(read, [how(TaskKind::Command, TaskOutcome::Failed), how(TaskKind::Wait, TaskOutcome::Finished)]);
    let wait_ended = ended[1].wait.as_ref().and_then(|wait| wait.ended).map(|ended| ended.how);
    assert_eq!(wait_ended, Some(WaitEnd::ChecksPassed));
    assert_eq!(own.kind(), "own_turn");
    assert_eq!(serde_json::to_string(&own).unwrap(), stored);
    let unknown = EventBody::OwnTurn(OwnTurn::Unknown);
    assert_eq!(serde_json::from_str::<EventBody>(later).unwrap(), unknown);
    let board = stored.replace(r#"{"on":"branch","number":12,"until":"all_checks"}"#, r#"{"on":"a_board","board":3}"#);
    let EventBody::OwnTurn(OwnTurn::TaskEnded { ended }) = serde_json::from_str(&board).unwrap() else { panic!() };
    assert_eq!(ended[1].wait.as_ref().map(|wait| &wait.watching), Some(&Watching::Unknown));
}

/// The hub keeps conversation mode's start in the form it sends.
#[test]
fn conversation_mode_turned_on_keeps_its_form() {
    let started = EventBody::ConversationStarted {
        device: protocol::model::ClientDevice { id: "d1".into(), name: "Mac Studio".into() },
    };
    let stored = r#"{"type":"conversation_started","device":{"id":"d1","name":"Mac Studio"}}"#;
    assert_eq!(serde_json::to_string(&started).unwrap(), stored);
    assert_eq!(serde_json::from_str::<EventBody>(stored).unwrap(), started);
}
