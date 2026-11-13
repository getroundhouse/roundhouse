//! Finding text in a chat with every kind of call in it, to check that what each kind draws is what finding counts.

use std::collections::BTreeSet;
use std::sync::Arc;

use gpui_kit::PlatformTextSystem;
use protocol::event::{EventBody, ExitReason, FailureKind, NoticeLevel, RequestFailure, TurnOutcome};
use protocol::model::SessionStatus;

use super::world::World;
use super::worlds::chat_world;
use super::{capture, file_chats, subagent_chats, tool_chats};

/// What finding counts in each kind of call is what the call draws, so every match drawn is one that can be gone to,
/// and the count is of what's there.
#[test]
fn what_each_kind_of_call_draws_is_what_finding_counts() {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    // Letters most texts have, a number, what's in a path, and a space, which JSON and code have plenty of.
    let queries = ["e", "a", "1", "/", " "];
    let mut problems = Vec::new();
    let mut texts_counted = 0;

    for capture::FoundBothWays { query, drawn, counted } in
        capture::found_both_ways(every_kind_of_call, &queries, &text_system)
    {
        assert!(!counted.is_empty(), "{query:?} is in some of the texts");
        texts_counted += counted.len();
        let texts: BTreeSet<&String> = drawn.keys().chain(counted.keys()).collect();
        for text in texts {
            let (drawn, counted) = (drawn.get(text).copied().unwrap_or(0), counted.get(text).copied().unwrap_or(0));
            if drawn != counted {
                problems.push(format!("{query:?} in {text}: {drawn} drawn, {counted} counted"));
            }
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(texts_counted > 100, "with plenty to compare: {texts_counted}");
}

/// A chat with every kind of call the transcript draws in it, for finding in.
fn every_kind_of_call() -> World {
    let chats = [
        tool_chats::commands(),
        tool_chats::any_tool(),
        tool_chats::advisor(),
        tool_chats::web(),
        file_chats::files(),
        file_chats::file_contents(),
        file_chats::searches(),
        file_chats::failures(),
        subagent_chats::subagents(false),
        what_went_wrong(),
    ];
    chat_world(SessionStatus::Idle, chats.concat())
}

/// A notice, a request that failed, a turn that failed, and the agent stopping unexpectedly, each saying what went
/// wrong.
fn what_went_wrong() -> Vec<EventBody> {
    let failed = EventBody::TurnEnded {
        outcome: TurnOutcome::Error,
        result: None,
        errors: vec!["API Error: 529 the servers are overloaded, try again in a minute".into()],
        duration_ms: Some(1_200),
        cost_usd: None,
        num_turns: None,
        started_at_ms: None,
        models: Vec::new(),
        tokens: None,
        only_compacted: false,
        request_failed: false,
    };
    let stopped = EventBody::AgentExited {
        reason: ExitReason::Unexpected,
        code: Some(1),
        signal: None,
        stderr_tail: vec!["Error: the session file at /tmp/claude/session.json is locked".into()],
    };
    let notice = EventBody::Notice {
        level: NoticeLevel::Warning,
        message: "The agent's CLI is 2 versions behind".into(),
        parent_tool_use_id: None,
    };
    let limit = RequestFailure {
        message: super::failure_chats::LIMIT_REACHED.into(),
        kind: FailureKind::Limit,
        resets_at_ms: None,
    };
    vec![notice, EventBody::RequestFailed { parent_tool_use_id: None, failure: limit }, failed, stopped]
}
