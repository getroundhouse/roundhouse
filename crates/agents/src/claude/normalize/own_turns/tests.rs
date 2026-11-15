use protocol::capability::turns::OwnTurn;
use protocol::event::{AgentTask, EndedTask, EventBody, TaskKind, TaskOutcome};
use serde_json::json;

use super::super::Normalizer;

fn listed(tasks: &[(&str, &str, &str)]) -> String {
    let tasks: Vec<_> = tasks
        .iter()
        .map(|(id, task_type, description)| json!({"task_id":id,"task_type":task_type,"description":description}))
        .collect();
    json!({"type":"system","subtype":"background_tasks_changed","tasks":tasks}).to_string()
}

fn started(task: &str, call: &str, task_type: &str) -> String {
    json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":call,"task_type":task_type})
        .to_string()
}

fn ended(task: &str, call: &str, status: &str) -> String {
    json!({"type":"system","subtype":"task_notification","task_id":task,"tool_use_id":call,"status":status}).to_string()
}

fn turn_ended() -> String {
    json!({"type":"result","subtype":"success","is_error":false,"result":"Done."}).to_string()
}

fn prompt_started() -> String {
    json!({"type":"command_lifecycle","command_uuid":"u1","state":"started"}).to_string()
}

/// A call `subagent`'s subagent makes, as the CLI streams it.
fn subagents_call(subagent: &str, call: &str) -> String {
    json!({"type":"assistant","parent_tool_use_id":subagent,
           "message":{"id":"m9","role":"assistant","content":[{"type":"tool_use","id":call,"name":"Bash","input":{}}]}})
    .to_string()
}

/// The result of a call `subagent`'s subagent made, which the CLI sends before the task the call started ends.
fn subagents_result(subagent: &str, call: &str) -> String {
    json!({"type":"user","parent_tool_use_id":subagent,
           "message":{"role":"user","content":[{"type":"tool_result","tool_use_id":call,"content":"Running in the background."}]}})
    .to_string()
}

fn requesting() -> String {
    json!({"type":"system","subtype":"status","status":"requesting","session_id":"s"}).to_string()
}

fn init() -> String {
    json!({"type":"system","subtype":"init","session_id":"s"}).to_string()
}

fn task(id: &str, kind: TaskKind, description: &str, call: &str, outcome: TaskOutcome) -> EndedTask {
    let task = AgentTask {
        id: id.into(),
        description: Some(description.into()),
        kind,
        call: Some(call.into()),
        subagent: None,
    };
    EndedTask { task, outcome, wait: None }
}

fn own_turn(ended: Vec<EndedTask>) -> EventBody {
    EventBody::OwnTurn(OwnTurn::TaskEnded { ended })
}

/// As the CLI sends them: a task's notification once it ends, then the shorter list, then the turn's `init`. Each says
/// what its list said of it, and how it ended.
#[test]
fn a_turn_the_agent_starts_by_itself_says_which_tasks_it_heard_had_ended() {
    let mut normalizer = Normalizer::default();
    normalizer.line(&listed(&[("b1", "local_bash", "Run the tests"), ("a2", "local_agent", "Review the change")]));
    normalizer.line(&started("b1", "c1", "local_bash"));
    normalizer.line(&started("a2", "c2", "local_agent"));
    normalizer.line(&turn_ended());

    normalizer.line(&ended("b1", "c1", "failed"));
    normalizer.line(&ended("a2", "c2", "completed"));
    normalizer.line(&listed(&[]));
    let started = normalizer.line(&init());

    let ended = vec![
        task("b1", TaskKind::Command, "Run the tests", "c1", TaskOutcome::Failed),
        task("a2", TaskKind::Subagent, "Review the change", "c2", TaskOutcome::Finished),
    ];
    assert_eq!(started.after, Some(own_turn(ended)), "after its init");
}

/// One started again with `--resume` lists its tasks again before any prompt, and a wait that ends then starts a turn.
#[test]
fn a_process_started_again_starts_a_turn_for_a_task_that_ends_before_any_prompt() {
    let mut normalizer = Normalizer::default();
    normalizer.line(&listed(&[("k3", "mcp_task", "roundhouse/wait_for_pull_requests")]));

    normalizer.line(&ended("k3", "c3", "completed"));
    let started = normalizer.line(&init());

    let wait = task("k3", TaskKind::Wait, "roundhouse/wait_for_pull_requests", "c3", TaskOutcome::Finished);
    assert_eq!(started.after, Some(own_turn(vec![wait])));
}

/// A task that ends mid-turn is taken in by the turn's next request, and one that ends after its last request starts
/// a turn of its own once it ends.
#[test]
fn a_task_that_ended_mid_turn_starts_a_turn_only_when_no_request_took_it_in() {
    let mut normalizer = Normalizer::default();
    normalizer.line(&listed(&[("b1", "local_bash", "Run the tests"), ("b2", "local_bash", "Build")]));
    normalizer.line(&prompt_started());
    let prompted = normalizer.line(&init());
    normalizer.line(&requesting());
    normalizer.line(&ended("b1", "c1", "completed"));
    normalizer.line(&requesting());
    normalizer.line(&ended("b2", "c2", "failed"));
    normalizer.line(&turn_ended());

    let after = normalizer.line(&init());

    assert_eq!(prompted.after, None);
    let build = task("b2", TaskKind::Command, "Build", "c2", TaskOutcome::Failed);
    assert_eq!(after.after, Some(own_turn(vec![build])), "only what no request took in");
}

/// A prompt the CLI starts on starts the next turn, whatever ended before it, and one taken mid-turn goes with that
/// turn, so the turn after it can still be the agent's own.
#[test]
fn a_prompt_starts_the_turn_after_it_but_not_one_after_the_turn_it_was_taken_in() {
    let mut normalizer = Normalizer::default();
    normalizer.line(&listed(&[("b1", "local_bash", "Run the tests"), ("b2", "local_bash", "Build")]));
    normalizer.line(&turn_ended());
    normalizer.line(&ended("b1", "c1", "completed"));
    normalizer.line(&prompt_started());
    let prompted = normalizer.line(&init());
    normalizer.line(&requesting());
    normalizer.line(&prompt_started());
    normalizer.line(&requesting());
    normalizer.line(&turn_ended());
    normalizer.line(&ended("b2", "c2", "completed"));

    let after_the_steered_turn = normalizer.line(&init());

    assert_eq!(prompted.after, None, "a prompt started it");
    let build = task("b2", TaskKind::Command, "Build", "c2", TaskOutcome::Finished);
    assert_eq!(after_the_steered_turn.after, Some(own_turn(vec![build])), "the prompt was taken mid-turn");
}

/// A task no list held ran in the foreground, as a subagent's commands do, and the call it ran for took it in, so a
/// turn the agent starts by itself doesn't name it, though no request of the agent's came after it.
#[test]
fn a_task_no_list_held_is_not_named() {
    let mut normalizer = Normalizer::default();
    normalizer.line(&listed(&[("a1", "local_agent", "Review the change")]));
    normalizer.line(&started("a1", "c1", "local_agent"));
    normalizer.line(&turn_ended());
    normalizer.line(&started("b9", "c9", "local_bash"));
    normalizer.line(&ended("b9", "c9", "completed"));

    normalizer.line(&ended("a1", "c1", "completed"));
    let started = normalizer.line(&init());

    let review = task("a1", TaskKind::Subagent, "Review the change", "c1", TaskOutcome::Finished);
    assert_eq!(started.after, Some(own_turn(vec![review])), "only the listed one");
}

/// A subagent's command in the background is on the agent's list, but the CLI tells the subagent how it ended, so a
/// turn the agent starts by itself doesn't name it, though no request of the agent's came after it. The call's result
/// comes before its end, so who made the call is known only until then.
#[test]
fn a_task_a_subagents_call_started_is_not_named() {
    let mut normalizer = Normalizer::default();
    normalizer.line(&listed(&[("a1", "local_agent", "Review the change")]));
    normalizer.line(&started("a1", "c1", "local_agent"));
    normalizer.line(&turn_ended());
    normalizer.line(&subagents_call("c1", "c9"));
    normalizer.line(&listed(&[("a1", "local_agent", "Review the change"), ("b9", "local_bash", "Run the tests")]));
    normalizer.line(&started("b9", "c9", "local_bash"));
    normalizer.line(&subagents_result("c1", "c9"));
    normalizer.line(&ended("b9", "c9", "completed"));
    normalizer.line(&listed(&[("a1", "local_agent", "Review the change")]));

    normalizer.line(&ended("a1", "c1", "completed"));
    let started = normalizer.line(&init());

    let review = task("a1", TaskKind::Subagent, "Review the change", "c1", TaskOutcome::Finished);
    assert_eq!(started.after, Some(own_turn(vec![review])), "only the agent's own");
}
