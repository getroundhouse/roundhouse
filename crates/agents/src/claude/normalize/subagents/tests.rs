use protocol::event::{EventBody, SubagentOutcome};
use serde_json::{Value, json};

use crate::claude::normalize::{Normalizer, Parsed};

/// What the CLI says as a task starts, with the fields 2.1.319 to 2.1.322 sent.
fn started(task: &str, task_type: &str, in_background: bool) -> Value {
    json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":"agent","description":"Find the tests",
        "task_type":task_type,"subagent_type":"Explore","prompt":"Find where the tests live.",
        "is_backgrounded":in_background,"spawn_depth":1,"session_id":"s","uuid":"u"})
}

/// What it says as a task ends. One of the subagents' didn't name the call.
fn notification(task: &str, status: &str) -> Value {
    json!({"type":"system","subtype":"task_notification","task_id":task,"status":status,"summary":"They're in tests/.",
        "output_file":"/tmp/out","session_id":"s","uuid":"u"})
}

fn event(normalizer: &mut Normalizer, line: &Value) -> EventBody {
    let Parsed::Event(body) = normalizer.line(&line.to_string()).parsed else { panic!("{line}") };
    body
}

#[test]
fn a_subagent_starts_with_what_it_was_asked_and_ends_with_its_report() {
    let mut normalizer = Normalizer::default();

    let start = event(&mut normalizer, &started("t1", "local_agent", true));
    let end = event(&mut normalizer, &notification("t1", "completed"));

    assert_eq!(
        start,
        EventBody::SubagentStarted {
            call: "agent".into(),
            kind: Some("Explore".into()),
            description: Some("Find the tests".into()),
            prompt: Some("Find where the tests live.".into()),
            in_background: true,
        }
    );
    let report = Some("They're in tests/.".into());
    let ended = EventBody::SubagentEnded {
        call: "agent".into(),
        outcome: SubagentOutcome::Finished,
        report,
        context_tokens: None,
    };
    assert_eq!(end, ended);
    assert!(
        matches!(event(&mut normalizer, &notification("t1", "completed")), EventBody::AgentOther { .. }),
        "it ended once"
    );
}

#[test]
fn a_stopped_subagent_reports_nothing() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", false));

    let end = event(&mut normalizer, &notification("t1", "stopped"));

    let ended = EventBody::SubagentEnded {
        call: "agent".into(),
        outcome: SubagentOutcome::Stopped,
        report: None,
        context_tokens: None,
    };
    assert_eq!(end, ended);
}

/// What it says as a task works, with the fields 2.1.319 to 2.1.322 sent for a subagent's.
fn progress(task: &str, tokens: u64) -> Value {
    json!({"type":"system","subtype":"task_progress","task_id":task,"run_id":"r","tool_use_id":"agent",
        "description":"Reading docs/gpui.md","subagent_type":"Explore",
        "usage":{"total_tokens":tokens,"tool_uses":9,"duration_ms":7328},"last_tool_name":"Read","session_id":"s",
        "uuid":"u"})
}

#[test]
fn a_subagent_says_how_many_tokens_its_conversation_holds_as_it_works_and_as_it_ends() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", true));

    let working = event(&mut normalizer, &progress("t1", 37_386));
    let mut ending = notification("t1", "completed");
    ending["usage"] = json!({"total_tokens":41_020,"tool_uses":9,"duration_ms":9000});
    let end = event(&mut normalizer, &ending);

    assert_eq!(working, EventBody::SubagentContext { call: "agent".into(), tokens: 37_386 });
    assert!(matches!(end, EventBody::SubagentEnded { context_tokens: Some(41_020), .. }), "{end:?}");
    assert!(
        matches!(event(&mut normalizer, &progress("t1", 41_020)), EventBody::AgentOther { .. }),
        "it's ended, so nothing is working"
    );
}

#[test]
fn progress_that_counts_no_tokens_or_isnt_a_subagents_stays_as_it_was() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", true));
    // A tool's task says how far it's got too, counting 0.
    event(&mut normalizer, &started("t2", "mcp_task", true));
    let mut uncounted = progress("t1", 0);
    uncounted["usage"] = json!({"tool_uses":9});

    let uncounted = event(&mut normalizer, &uncounted);
    let a_tools = event(&mut normalizer, &progress("t2", 0));

    assert!(matches!(uncounted, EventBody::AgentOther { kind, .. } if kind == "system/task_progress"));
    assert!(matches!(a_tools, EventBody::AgentOther { kind, .. } if kind == "system/task_progress"));
}

#[test]
fn a_task_that_isnt_a_subagent_stays_as_it_was() {
    let mut normalizer = Normalizer::default();

    let start = event(&mut normalizer, &started("t1", "local_bash", true));
    let end = event(&mut normalizer, &notification("t1", "completed"));

    assert!(matches!(start, EventBody::AgentOther { kind, .. } if kind == "system/task_started"));
    assert!(matches!(end, EventBody::AgentOther { kind, .. } if kind == "system/task_notification"));
}

#[test]
fn the_end_of_a_turn_leaves_the_block_a_subagent_in_the_background_is_streaming() {
    let delta = |message: &str, call: &str| {
        json!({"type":"stream_event","api_message_id":message,"parent_tool_use_id":call,
            "event":{"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"x"}}})
    };
    let first_index = |normalizer: &mut Normalizer, message: &str, call: &str| {
        let finished = json!({"type":"assistant","parent_tool_use_id":call,
            "message":{"id":message,"content":[{"type":"text","text":"x"}]}});
        let EventBody::Assistant { first_index, .. } = event(normalizer, &finished) else { panic!("{finished}") };
        first_index
    };
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", true));
    let mut in_front = started("t2", "local_agent", false);
    in_front["tool_use_id"] = json!("other");
    event(&mut normalizer, &in_front);
    normalizer.line(&delta("behind", "agent").to_string());
    normalizer.line(&delta("in_front", "other").to_string());

    normalizer.line(&json!({"type":"result","subtype":"success","is_error":false}).to_string());

    assert_eq!(first_index(&mut normalizer, "behind", "agent"), Some(1), "it carries on");
    assert_eq!(first_index(&mut normalizer, "in_front", "other"), None, "it went with the turn");
}
