use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::TaskId;
use serde_json::json;

use super::super::{Normalizer, Parsed, parse_line};

#[test]
fn background_tasks_are_listed_whole_each_time_they_change() {
    let line = json!({"type":"system","subtype":"background_tasks_changed","tasks":[
        {"task_id":"b1","run_id":"r1","task_type":"local_bash","description":"Run the tests"},
        {"task_id":"a2","run_id":"r2","task_type":"local_agent","subagent_type":"Explore"}]});
    let tasks = vec![
        AgentTask { id: "b1".into(), description: Some("Run the tests".into()), kind: TaskKind::Command, call: None },
        AgentTask { id: "a2".into(), description: None, kind: TaskKind::Subagent, call: None },
    ];
    assert_eq!(parse_line(&line.to_string()), Parsed::Event(EventBody::AgentTasks { tasks, stopped: vec![] }));
    let done = json!({"type":"system","subtype":"background_tasks_changed","tasks":[]}).to_string();
    assert_eq!(parse_line(&done), Parsed::Event(EventBody::AgentTasks { tasks: vec![], stopped: vec![] }));
}

/// As the CLI describes an MCP task, by the server's tool whose call started it, in its list from the first. Only
/// the worker's `wait_for_pull_request` is a wait. Another tool's task, the same tool of another server, and a task
/// that isn't a tool's are what they always were.
#[test]
fn an_mcp_task_of_the_worker_tool_that_waits_is_a_wait() {
    let kind = |task_type: &str, description: &str| {
        let task = json!({"task_id":"k1","run_id":"r1","task_type":task_type,"description":description});
        let line = json!({"type":"system","subtype":"background_tasks_changed","tasks":[task]}).to_string();
        let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = parse_line(&line) else { panic!("a list") };
        tasks[0].kind
    };

    assert_eq!(kind("mcp_task", "roundhouse/wait_for_pull_request"), TaskKind::Wait);
    assert_eq!(kind("mcp_task", "roundhouse/wait_for_pull_requests"), TaskKind::Wait);
    assert_eq!(kind("mcp_task", "roundhouse/get_pull_request"), TaskKind::Command);
    assert_eq!(kind("mcp_task", "elsewhere/wait_for_pull_request"), TaskKind::Command);
    assert_eq!(kind("local_bash", "roundhouse/wait_for_pull_request"), TaskKind::Command);
}

#[test]
fn a_task_that_leaves_the_list_stopped_rather_than_finished_says_so() {
    let ended = |id: &str, status: &str| {
        json!({"type":"system","subtype":"task_notification","task_id":id,"status":status}).to_string()
    };
    let listed = |ids: &[&str]| {
        let tasks: Vec<_> = ids.iter().map(|id| json!({"task_id":id})).collect();
        json!({"type":"system","subtype":"background_tasks_changed","tasks":tasks}).to_string()
    };
    let stopped = |parsed: Parsed| {
        let Parsed::Event(EventBody::AgentTasks { stopped, .. }) = parsed else { panic!("{parsed:?}") };
        stopped
    };
    let mut normalizer = Normalizer::default();

    // As the CLI does on SIGTERM: the notice first, then the list without it.
    normalizer.line(&ended("a", "stopped"));
    normalizer.line(&ended("b", "completed"));
    assert_eq!(stopped(normalizer.line(&listed(&["a", "c"])).parsed), Vec::<TaskId>::new(), "still listed");
    assert_eq!(stopped(normalizer.line(&listed(&["c"])).parsed), vec![TaskId::from("a")]);
    assert_eq!(stopped(normalizer.line(&listed(&[])).parsed), Vec::<TaskId>::new(), "said once");
}

/// The lines the CLI sends around a `Monitor` call, in its order: the call, the list with its task, then the
/// `task_started` that names the call.
#[test]
fn a_task_a_monitor_call_started_is_a_monitor_from_the_line_that_says_so() {
    let call = |name: &str, id: &str| {
        json!({"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":id,"name":name,"input":{}}]}})
            .to_string()
    };
    let listed = |ids: &[&str]| {
        let tasks: Vec<_> = ids.iter().map(|id| json!({"task_id":id,"task_type":"local_bash"})).collect();
        json!({"type":"system","subtype":"background_tasks_changed","tasks":tasks}).to_string()
    };
    let started = |task: &str, call: &str| {
        json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":"local_bash"})
            .to_string()
    };
    let tasks = |parsed: Parsed| {
        let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = parsed else { panic!("{parsed:?}") };
        tasks
    };
    let kinds = |parsed: Parsed| tasks(parsed).into_iter().map(|task| (task.id, task.kind)).collect::<Vec<_>>();
    let calls = |parsed: Parsed| tasks(parsed).into_iter().map(|task| (task.id, task.call)).collect::<Vec<_>>();
    let (command, monitor) = (TaskKind::Command, TaskKind::Monitor);
    let mut normalizer = Normalizer::default();

    normalizer.line(&call("Bash", "c1"));
    assert_eq!(kinds(normalizer.line(&listed(&["t1"])).parsed), [("t1".into(), command)]);
    let other = Parsed::Event(EventBody::AgentOther { kind: "system/task_started".into(), parent_tool_use_id: None });
    assert_eq!(normalizer.line(&started("t1", "c1")).parsed, other, "a command's call changes nothing shown");

    normalizer.line(&call("Monitor", "c2"));
    let next = calls(normalizer.line(&listed(&["t1", "t2"])).parsed);
    assert_eq!(next, [("t1".into(), Some("c1".into())), ("t2".into(), None)], "the next list names the command's call");
    let relisted: Vec<_> = tasks(normalizer.line(&started("t2", "c2")).parsed)
        .into_iter()
        .map(|task| (task.id, task.kind, task.call.map(|call| call.to_string())))
        .collect();
    let named = |call: &str| Some(call.to_string());
    let known = [("t1".into(), command, named("c1")), ("t2".into(), monitor, named("c2"))];
    assert_eq!(relisted, known, "the list again, now it's known");
    assert_eq!(kinds(normalizer.line(&listed(&["t2"])).parsed), [("t2".into(), monitor)], "and in every list after");

    // The CLI can still list a task after saying it ended, so it's a monitor until a list leaves it out.
    let ended = json!({"type":"system","subtype":"task_notification","task_id":"t2","status":"completed"});
    normalizer.line(&ended.to_string());
    assert_eq!(kinds(normalizer.line(&listed(&["t2"])).parsed), [("t2".into(), monitor)]);
    assert_eq!(kinds(normalizer.line(&listed(&[])).parsed), []);
    assert_eq!(kinds(normalizer.line(&listed(&["t2"])).parsed), [("t2".into(), command)], "its ID is free again");
}

#[test]
fn a_monitor_that_starts_before_its_listed_is_one_in_its_first_list() {
    let mut normalizer = Normalizer::default();
    let call = json!({"type":"assistant","message":{"id":"m","content":[
        {"type":"tool_use","id":"c1","name":"Monitor","input":{}}]}});
    let started = json!({"type":"system","subtype":"task_started","task_id":"t1","tool_use_id":"c1"});
    let listed = json!({"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"t1"}]});

    normalizer.line(&call.to_string());
    let start = normalizer.line(&started.to_string()).parsed;
    let list = normalizer.line(&listed.to_string()).parsed;

    assert_eq!(
        start,
        Parsed::Event(EventBody::AgentOther { kind: "system/task_started".into(), parent_tool_use_id: None })
    );
    let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = list else { panic!("{list:?}") };
    assert_eq!((tasks[0].kind, tasks[0].call.clone()), (TaskKind::Monitor, Some("c1".into())));
}

#[test]
fn a_list_of_background_tasks_it_cant_read_doesnt_say_theyre_done() {
    let line = json!({"type":"system","subtype":"background_tasks_changed"}).to_string();
    assert_eq!(
        parse_line(&line),
        Parsed::Event(EventBody::AgentOther {
            kind: "system/background_tasks_changed".into(),
            parent_tool_use_id: None
        })
    );
}

/// A call is only a monitor's while it's open. One that was refused has had its result, so nothing it's named for
/// afterwards is a monitor.
#[test]
fn a_monitor_call_that_was_answered_without_starting_a_task_starts_no_monitor() {
    let mut normalizer = Normalizer::default();
    let call = json!({"type":"assistant","message":{"id":"m","content":[
        {"type":"tool_use","id":"c1","name":"Monitor","input":{}}]}});
    let refused = json!({"type":"user","message":{"content":[
        {"type":"tool_result","tool_use_id":"c1","is_error":true,"content":"The user doesn't want to proceed"}]}});
    let listed = json!({"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"t1"}]});
    let started = json!({"type":"system","subtype":"task_started","task_id":"t1","tool_use_id":"c1"});

    normalizer.line(&call.to_string());
    normalizer.line(&refused.to_string());
    normalizer.line(&listed.to_string());
    let start = normalizer.line(&started.to_string()).parsed;

    assert_eq!(
        start,
        Parsed::Event(EventBody::AgentOther { kind: "system/task_started".into(), parent_tool_use_id: None })
    );
    let again = normalizer.line(&listed.to_string()).parsed;
    let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = again else { panic!("{again:?}") };
    assert_eq!(tasks[0].kind, TaskKind::Command);
}

/// The lines the CLI sends around a subagent's call in the background, in its order: the call, the list with its
/// task, then the `task_started` that names the call, which is the subagent's start, and the list again after it.
#[test]
fn a_subagent_in_the_background_is_listed_with_its_call_once_its_start_names_it() {
    let mut normalizer = Normalizer::default();
    let call = json!({"type":"assistant","message":{"id":"m","content":[
        {"type":"tool_use","id":"c1","name":"Agent","input":{"description":"Find the tests"}}]}});
    let listed = |ids: &[&str]| {
        let tasks: Vec<_> = ids
            .iter()
            .map(|id| json!({"task_id":id,"task_type":"local_agent","description":"Find the tests"}))
            .collect();
        json!({"type":"system","subtype":"background_tasks_changed","tasks":tasks}).to_string()
    };
    let started = json!({"type":"system","subtype":"task_started","task_id":"a1","tool_use_id":"c1",
        "task_type":"local_agent","description":"Find the tests","is_backgrounded":true});
    let ended = json!({"type":"system","subtype":"task_notification","task_id":"a1","status":"completed"});
    let calls = |body: Option<EventBody>| {
        let Some(EventBody::AgentTasks { tasks, .. }) = body else { panic!("{body:?}") };
        tasks.into_iter().map(|task| (task.id, task.kind, task.call)).collect::<Vec<_>>()
    };
    let subagent = TaskKind::Subagent;

    normalizer.line(&call.to_string());
    let first = normalizer.line(&listed(&["a1"]));
    let start = normalizer.line(&started.to_string());

    assert_eq!(calls(Some(event(first.parsed))), [("a1".into(), subagent, None)], "its call isn't known yet");
    assert!(matches!(start.parsed, Parsed::Event(EventBody::SubagentStarted { .. })), "{:?}", start.parsed);
    assert_eq!(calls(start.relisted), [("a1".into(), subagent, Some("c1".into()))], "the list again, after its start");
    let next = normalizer.line(&listed(&["a1"]));
    assert_eq!(calls(Some(event(next.parsed))), [("a1".into(), subagent, Some("c1".into()))], "and every list after");

    // The CLI can still list a task after saying it ended, so it keeps its call until a list leaves it out.
    normalizer.line(&ended.to_string());
    let after_its_end = normalizer.line(&listed(&["a1"]));
    assert_eq!(calls(Some(event(after_its_end.parsed))), [("a1".into(), subagent, Some("c1".into()))]);
    normalizer.line(&listed(&[]));
    let reused = normalizer.line(&listed(&["a1"]));
    assert_eq!(calls(Some(event(reused.parsed))), [("a1".into(), subagent, None)], "its ID is free again");
}

/// Most tasks end without being listed, such as a command in the foreground, so nothing of theirs is kept.
#[test]
fn a_task_that_ends_without_being_listed_is_forgotten() {
    let mut normalizer = Normalizer::default();
    let started = json!({"type":"system","subtype":"task_started","task_id":"t1","tool_use_id":"c1",
        "task_type":"local_bash"});
    let ended = json!({"type":"system","subtype":"task_notification","task_id":"t1","status":"completed"});
    let listed = json!({"type":"system","subtype":"background_tasks_changed","tasks":[{"task_id":"t1"}]});

    normalizer.line(&started.to_string());
    normalizer.line(&ended.to_string());
    let list = normalizer.line(&listed.to_string()).parsed;

    let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = list else { panic!("{list:?}") };
    assert_eq!(tasks[0].call, None);
}

fn event(parsed: Parsed) -> EventBody {
    let Parsed::Event(body) = parsed else { panic!("{parsed:?}") };
    body
}
