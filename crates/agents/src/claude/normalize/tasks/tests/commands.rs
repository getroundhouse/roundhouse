use protocol::event::{AgentTask, EventBody, TaskChange, TaskKind};
use serde_json::{Value, json};

use super::super::super::{Normalizer, Parsed};
use super::listed::event;

/// The lines the CLI sends around a command a subagent starts in the background, with no word on what it's for: the
/// CLI describes it as the whole of itself, and lists it one line before `task_started` names the call.
#[test]
fn a_subagents_undescribed_command_says_what_it_runs_and_is_part_of_its_work_until_it_ends() {
    let script = "python3 - <<'EOF'\nprint('done')\nEOF";
    for in_background in [true, false] {
        let mut normalizer = Normalizer::default();
        let mut line = |value: Value| normalizer.line(&value.to_string());
        let started = |task: &str, call: &str, kind: &str, in_background: bool| {
            json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":kind,
                "is_backgrounded":in_background})
        };
        let subagent = json!({"task_id":"a1","task_type":"local_agent"});
        let command = json!({"task_id":"b1","task_type":"local_bash","description":script});
        let listed = |with_the_command: bool| {
            let its_own = in_background.then_some(&subagent);
            let tasks: Vec<_> = its_own.into_iter().chain(with_the_command.then_some(&command)).collect();
            json!({"type":"system","subtype":"background_tasks_changed","tasks":tasks})
        };
        let says = |body: Option<EventBody>| {
            let Some(EventBody::AgentTasks { tasks, .. }) = body else { panic!("{body:?}") };
            let task = tasks.into_iter().find(|task| task.id.as_str() == "b1").unwrap();
            (task.description, task.command, task.part_of)
        };

        line(json!({"type":"assistant","message":{"id":"m","content":[
            {"type":"tool_use","id":"c1","name":"Agent","input":{}}]}}));
        line(listed(false));
        line(started("a1", "c1", "local_agent", in_background));
        line(json!({"type":"assistant","parent_tool_use_id":"c1","agent_id":"a1","message":{"id":"n","content":[
            {"type":"tool_use","id":"c2","name":"Bash","input":{"command":script,"run_in_background":true}}]}}));
        line(listed(true));
        let start = line(started("b1", "c2", "local_bash", true));
        line(json!({"type":"user","parent_tool_use_id":"c1","agent_id":"a1","message":{"content":[
            {"type":"tool_result","tool_use_id":"c2","content":"Command running in background with ID: b1"}]}}));
        let next = line(listed(true));
        let end = line(json!({"type":"system","subtype":"task_notification","task_id":"a1","status":"completed"}));
        let again = line(started("a1", "c3", "local_agent", in_background));

        let runs = Some(script.to_string());
        let of_it = (None, runs.clone(), Some("a1".into()));
        assert_eq!(says(Some(event(start.parsed))), of_it, "the list again, now it's known");
        assert_eq!(says(Some(event(next.parsed))), of_it, "and in every list after, its call answered");
        assert!(matches!(end.parsed, Parsed::Event(EventBody::SubagentEnded { .. })), "{:?}", end.parsed);
        assert_eq!(says(end.after), (None, runs, None), "left with the agent as the subagent ends");
        assert_eq!(says(again.after), of_it, "the subagent's again as it carries on");
    }
}

/// A command whose caller said what it's for keeps that, and the agent's own is part of no subagent's work.
#[test]
fn the_agents_own_undescribed_command_says_what_it_runs_and_a_described_one_what_its_for() {
    let mut normalizer = Normalizer::default();
    let mut line = |value: Value| normalizer.line(&value.to_string()).parsed;
    let started = |task: &str, call: &str| json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":"local_bash"});

    line(json!({"type":"assistant","message":{"id":"m","content":[
        {"type":"tool_use","id":"c1","name":"Bash","input":{"command":"ls"}},
        {"type":"tool_use","id":"c2","name":"Bash","input":{"command":"pwd","description":"Say where it is"}}]}}));
    line(json!({"type":"system","subtype":"background_tasks_changed","tasks":[
        {"task_id":"b1","task_type":"local_bash","description":"ls"},
        {"task_id":"b2","task_type":"local_bash","description":"Say where it is"}]}));
    let of_the_described = line(started("b2", "c2"));
    let list = event(line(started("b1", "c1")));

    let named = Parsed::Event(EventBody::TaskChanged { task: "b2".into(), change: TaskChange::Started });
    assert_eq!(of_the_described, named, "its call changes nothing shown, so the list isn't said again");
    let command = |id: &str, call: &str| AgentTask { call: Some(call.into()), ..AgentTask::new(id, TaskKind::Command) };
    let undescribed = AgentTask { command: Some("ls".into()), ..command("b1", "c1") };
    let described = AgentTask { description: Some("Say where it is".into()), ..command("b2", "c2") };
    assert_eq!(list, EventBody::AgentTasks { tasks: vec![undescribed, described], stopped: vec![] });
}

#[test]
fn a_subagents_described_command_is_part_of_its_work_from_the_line_that_names_its_call() {
    let mut normalizer = Normalizer::default();
    let mut line = |value: Value| normalizer.line(&value.to_string()).parsed;
    let started = |task: &str, call: &str, kind: &str| json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":kind});

    line(started("a1", "c1", "local_agent"));
    line(json!({"type":"assistant","parent_tool_use_id":"c1","agent_id":"a1","message":{"id":"m","content":[
        {"type":"tool_use","id":"c2","name":"Bash","input":{"command":"pwd","description":"Say where it is"}}]}}));
    line(json!({"type":"system","subtype":"background_tasks_changed","tasks":[
        {"task_id":"b1","task_type":"local_bash","description":"Say where it is"}]}));
    let list = event(line(started("b1", "c2", "local_bash")));

    let command = AgentTask {
        description: Some("Say where it is".into()),
        call: Some("c2".into()),
        part_of: Some("a1".into()),
        ..AgentTask::new("b1", TaskKind::Command)
    };
    assert_eq!(list, EventBody::AgentTasks { tasks: vec![command], stopped: vec![] });
}

/// A task the CLI starts again names its call, which can have been answered long before, so what's known of who
/// started it stays.
#[test]
fn a_subagents_task_started_again_under_an_answered_call_is_still_its_own() {
    let mut normalizer = Normalizer::default();
    let mut line = |value: Value| normalizer.line(&value.to_string()).parsed;
    let started = |task: &str, call: &str| json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":"local_agent"});
    let listed = json!({"type":"system","subtype":"background_tasks_changed","tasks":[
        {"task_id":"a1","task_type":"local_agent"},{"task_id":"a2","task_type":"local_agent"}]});

    line(started("a1", "c1"));
    line(json!({"type":"assistant","parent_tool_use_id":"c1","agent_id":"a1","message":{"id":"m","content":[
        {"type":"tool_use","id":"c2","name":"Agent","input":{}}]}}));
    line(started("a2", "c2"));
    line(json!({"type":"user","parent_tool_use_id":"c1","agent_id":"a1","message":{"content":[
        {"type":"tool_result","tool_use_id":"c2","content":"Async agent launched successfully."}]}}));
    line(started("a2", "c2"));
    let EventBody::AgentTasks { tasks, .. } = event(line(listed)) else { panic!("a list") };

    let parts: Vec<_> = tasks.into_iter().map(|task| task.part_of).collect();
    assert_eq!(parts, [None, Some("a1".into())]);
}
