//! What a chat's record says of its agent's work in the background, on a hub whose workspaces have stand-ins for their
//! workers, whose agents say what a test has them say. Turning the CLI's lines into those lists is its adapter's, as
//! `agents`' `claude::normalize::tasks::tests` say, and the daemon's `e2e/background.rs` has a real one do it.

use protocol::event::{AgentTask, EventBody, TaskKind};
use protocol::model::{SessionId, SessionStatus, ToolUseId};
use protocol::prompt::Prompt;

use crate::hub::Hub;
use crate::hub::stand_in_workers::{a_chat, a_turn, hub_with_stand_in_workers, the_agent_says};

/// A task of the kind `kind`, named `id`, and the call that started it, once the agent has said.
fn task(id: &str, kind: TaskKind, call: Option<&str>) -> AgentTask {
    AgentTask { id: id.into(), description: None, kind, call: call.map(ToolUseId::from), subagent: None }
}

fn listed(tasks: Vec<AgentTask>) -> EventBody {
    EventBody::AgentTasks { tasks, stopped: Vec::new() }
}

/// Each task the record of `chat` lists, by its ID, its kind and the call that started it, and the chat's status.
async fn recorded(hub: &Hub, chat: &SessionId) -> (Vec<(String, TaskKind, Option<String>)>, SessionStatus) {
    let session = hub.session_get(chat.clone()).await.unwrap();
    let tasks = session.background_tasks.into_iter();
    let each = tasks.map(|task| (task.id.to_string(), task.kind, task.call.map(|call| call.to_string()))).collect();
    (each, session.status)
}

/// The agent says what a task is, such as that a monitor's call started it, or which call started a subagent, only a
/// line after it first lists it, so it lists the same tasks again. The record takes each list whole, so what a client
/// shows of each, and opens from its line, is what the agent said last, though nothing but that changed.
#[tokio::test]
async fn the_record_takes_each_list_whole_though_only_what_a_task_is_has_changed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let (_, chat) = a_chat(&hub).await;
    a_turn(&hub, &asked, &chat.id, Prompt::new("watch the suite"), None).await;
    let (command, monitor, subagent) = (TaskKind::Command, TaskKind::Monitor, TaskKind::Subagent);
    let suite = || task("suite", command, Some("c1"));
    let says = |tasks| the_agent_says(&hub, &asked, &chat.id, [listed(tasks)]);

    says(vec![suite(), task("watch", command, None)]).await;
    let first = recorded(&hub, &chat.id).await;
    says(vec![suite(), task("watch", monitor, Some("c2"))]).await;
    let a_monitor = recorded(&hub, &chat.id).await;
    says(vec![suite(), task("watch", monitor, Some("c2")), task("look", subagent, None)]).await;
    says(vec![suite(), task("watch", monitor, Some("c2")), task("look", subagent, Some("c3"))]).await;
    let its_call = recorded(&hub, &chat.id).await;
    says(Vec::new()).await;

    let (background, named) = (SessionStatus::Background, |call: &str| Some(call.to_string()));
    let (suite, watch) = (("suite".to_string(), command, named("c1")), ("watch".to_string(), monitor, named("c2")));
    assert_eq!(first, (vec![suite.clone(), ("watch".into(), command, None)], background), "as it was first listed");
    assert_eq!(a_monitor, (vec![suite.clone(), watch.clone()], background), "a monitor's, once the agent said so");
    let look = ("look".to_string(), subagent, named("c3"));
    assert_eq!(its_call, (vec![suite, watch, look], background), "with the call that started the subagent");
    assert_eq!(recorded(&hub, &chat.id).await.0, [], "none, once the agent lists none");
}
