use protocol::event::{EventBody, TaskChange, TaskOutcome};
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
            subagent: Some("t1".into()),
            kind: Some("Explore".into()),
            description: Some("Find the tests".into()),
            prompt: Some("Find where the tests live.".into()),
            in_background: true,
        }
    );
    let report = Some("They're in tests/.".into());
    let ended = EventBody::SubagentEnded {
        subagent: "t1".into(),
        outcome: TaskOutcome::Finished,
        report,
        context_tokens: None,
    };
    assert_eq!(end, ended);
    assert!(
        matches!(event(&mut normalizer, &notification("t1", "completed")), EventBody::TaskChanged { .. }),
        "it ended once"
    );
}

/// A line from the subagent the call `call` started, naming the task it runs as as `agent_id` if `agent` does, as
/// each has since 2.1.322.
fn from(call: &str, agent: Option<&str>) -> Value {
    let mut line = json!({"type":"assistant","parent_tool_use_id":call,"session_id":"s",
        "message":{"id":"m","model":"claude-haiku-5-5","content":[{"type":"text","text":"x"}]}});
    if let Some(agent) = agent {
        line["agent_id"] = json!(agent);
    }
    line
}

fn subagent_of(normalizer: &mut Normalizer, line: &Value) -> Option<String> {
    event(normalizer, line).subagent().map(|subagent| subagent.as_str().to_owned())
}

/// A subagent is named by the task it runs as, which outlives the call that started it, and which its lines name as
/// `agent_id`. A line that names only the call is from the subagent the CLI said that call started, and from one whose
/// start wasn't heard, such as before the adapter started again, is named by the call.
#[test]
fn a_subagent_is_named_by_the_task_it_runs_as_which_its_lines_name_or_else_by_its_call() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", true));

    assert_eq!(subagent_of(&mut normalizer, &from("agent", Some("t1"))).as_deref(), Some("t1"));
    assert_eq!(subagent_of(&mut normalizer, &from("agent", None)).as_deref(), Some("t1"), "by the call it started");
    assert_eq!(subagent_of(&mut normalizer, &from("earlier", Some("t0"))).as_deref(), Some("t0"), "unheard");
    assert_eq!(subagent_of(&mut normalizer, &from("earlier", None)).as_deref(), Some("earlier"), "nor named");
    let own = json!({"type":"assistant","message":{"id":"m","content":[]}});
    assert_eq!(subagent_of(&mut normalizer, &own), None, "the agent's own");
}

/// The CLI starts a subagent's task again when the agent carries on with it after it ended, under the call that carries
/// on, such as a `SendMessage`, as the hub's own logs had it 78 times. Its lines still name the call that first started
/// it, and the subagent is the same.
#[test]
fn a_subagent_carried_on_with_starts_again_as_the_same_subagent() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", true));
    event(&mut normalizer, &notification("t1", "completed"));
    let mut again = started("t1", "local_agent", true);
    again["tool_use_id"] = json!("send");

    let start = event(&mut normalizer, &again);

    assert!(matches!(&start, EventBody::SubagentStarted { call, .. } if call.as_str() == "send"), "{start:?}");
    assert_eq!(start.subagent_started(), Some("t1".into()));
    assert_eq!(subagent_of(&mut normalizer, &from("agent", None)).as_deref(), Some("t1"));
    let ended = event(&mut normalizer, &notification("t1", "completed"));
    assert!(matches!(ended, EventBody::SubagentEnded { subagent, .. } if subagent.as_str() == "t1"));
}

/// A subagent's request for approval names the task it runs as, which is the subagent, whether or not the adapter
/// heard it start.
#[test]
fn a_subagent_s_request_names_it_by_the_task_it_runs_as() {
    let request = |agent: Option<&str>| {
        let mut line = json!({"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool",
            "tool_name":"Bash","input":{"command":"ls"},"tool_use_id":"toolu_1"}});
        if let Some(agent) = agent {
            line["request"]["agent_id"] = json!(agent);
        }
        line
    };
    let asked_by = |line: &Value| {
        let EventBody::ApprovalRequested { asked_by, .. } = event(&mut Normalizer::default(), line) else {
            panic!("{line}")
        };
        asked_by.map(|subagent| subagent.as_str().to_owned())
    };

    assert_eq!(asked_by(&request(Some("t1"))).as_deref(), Some("t1"));
    assert_eq!(asked_by(&request(None)), None, "the agent's own");
}

#[test]
fn a_stopped_subagent_reports_nothing() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", false));

    let end = event(&mut normalizer, &notification("t1", "stopped"));

    let ended = EventBody::SubagentEnded {
        subagent: "t1".into(),
        outcome: TaskOutcome::Stopped,
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

    assert_eq!(working, EventBody::SubagentContext { subagent: "t1".into(), tokens: 37_386 });
    assert!(matches!(end, EventBody::SubagentEnded { context_tokens: Some(41_020), .. }), "{end:?}");
    assert!(
        matches!(event(&mut normalizer, &progress("t1", 41_020)), EventBody::TaskChanged { .. }),
        "it's ended, so nothing is working"
    );
}

#[test]
fn progress_that_counts_no_tokens_or_isnt_a_subagents_is_only_a_change_to_its_task() {
    let mut normalizer = Normalizer::default();
    event(&mut normalizer, &started("t1", "local_agent", true));
    // A tool's task says how far it's got too, counting 0.
    event(&mut normalizer, &started("t2", "mcp_task", true));
    let mut uncounted = progress("t1", 0);
    uncounted["usage"] = json!({"tool_uses":9});

    let uncounted = event(&mut normalizer, &uncounted);
    let a_tools = event(&mut normalizer, &progress("t2", 0));

    let progressed = |task: &str| EventBody::TaskChanged { task: task.into(), change: TaskChange::Progressed };
    assert_eq!((uncounted, a_tools), (progressed("t1"), progressed("t2")));
}

#[test]
fn a_task_that_isnt_a_subagent_is_only_a_change_to_its_task() {
    let mut normalizer = Normalizer::default();

    let start = event(&mut normalizer, &started("t1", "local_bash", true));
    let end = event(&mut normalizer, &notification("t1", "completed"));

    assert_eq!(start, EventBody::TaskChanged { task: "t1".into(), change: TaskChange::Started });
    assert_eq!(end, EventBody::TaskChanged { task: "t1".into(), change: TaskChange::Ended });
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
