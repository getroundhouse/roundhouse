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
        AgentTask { id: "b1".into(), description: Some("Run the tests".into()), kind: TaskKind::Command },
        AgentTask { id: "a2".into(), description: None, kind: TaskKind::Subagent },
    ];
    assert_eq!(parse_line(&line.to_string()), Parsed::Event(EventBody::AgentTasks { tasks, stopped: vec![] }));
    let done = json!({"type":"system","subtype":"background_tasks_changed","tasks":[]}).to_string();
    assert_eq!(parse_line(&done), Parsed::Event(EventBody::AgentTasks { tasks: vec![], stopped: vec![] }));
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
    let kinds = |parsed: Parsed| {
        let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = parsed else { panic!("{parsed:?}") };
        tasks.into_iter().map(|task| (task.id, task.kind)).collect::<Vec<_>>()
    };
    let (command, monitor) = (TaskKind::Command, TaskKind::Monitor);
    let mut normalizer = Normalizer::default();

    normalizer.line(&call("Bash", "c1"));
    assert_eq!(kinds(normalizer.line(&listed(&["t1"])).parsed), [("t1".into(), command)]);
    let other = Parsed::Event(EventBody::AgentOther { kind: "system/task_started".into() });
    assert_eq!(normalizer.line(&started("t1", "c1")).parsed, other, "a command's start says nothing new");

    normalizer.line(&call("Monitor", "c2"));
    assert_eq!(kinds(normalizer.line(&listed(&["t1", "t2"])).parsed), [("t1".into(), command), ("t2".into(), command)]);
    let relisted = kinds(normalizer.line(&started("t2", "c2")).parsed);
    assert_eq!(relisted, [("t1".into(), command), ("t2".into(), monitor)], "the list again, now it's known");
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

    assert_eq!(start, Parsed::Event(EventBody::AgentOther { kind: "system/task_started".into() }));
    let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = list else { panic!("{list:?}") };
    assert_eq!(tasks[0].kind, TaskKind::Monitor);
}

#[test]
fn a_list_of_background_tasks_it_cant_read_doesnt_say_theyre_done() {
    let line = json!({"type":"system","subtype":"background_tasks_changed"}).to_string();
    assert_eq!(
        parse_line(&line),
        Parsed::Event(EventBody::AgentOther { kind: "system/background_tasks_changed".into() })
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

    assert_eq!(start, Parsed::Event(EventBody::AgentOther { kind: "system/task_started".into() }));
    let again = normalizer.line(&listed.to_string()).parsed;
    let Parsed::Event(EventBody::AgentTasks { tasks, .. }) = again else { panic!("{again:?}") };
    assert_eq!(tasks[0].kind, TaskKind::Command);
}
