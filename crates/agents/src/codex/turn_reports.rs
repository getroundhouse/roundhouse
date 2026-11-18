//! What Codex says of the agent's own turn that isn't an event: what the turn has changed, and its plan, each whole as
//! it stands, which the hub keeps the newest of on the session's record. A subagent's thread has turns of its own,
//! which say nothing of the agent's.

use protocol::capability::Report;
use protocol::capability::output::{Todo, TodoList, TodoStatus};
use serde::Deserialize;
use serde_json::Value;

use super::{items, running::Shared, subagents::Scope};
use crate::{AgentError, AgentOutput};

/// What the turn has changed, from its diff, whole, in git's form. Codex says it again whenever a change is made, and
/// often the same again, which isn't reported twice.
pub(super) async fn changes(shared: &Shared, params: &Value, scope: &Scope) {
    let Some(diff) = params["diff"].as_str().filter(|_| scope.parent.is_none()) else { return };
    {
        let mut state = shared.state.lock();
        if state.turn_diff.as_deref() == Some(diff) {
            return;
        }
        state.turn_diff = Some(diff.to_owned());
    }
    let files = items::turn_changes(diff);
    shared.emit(AgentOutput::Report(Report::TurnChanges { files })).await;
}

/// The plan Codex keeps as it works, from its plan tool, which some of its configurations have, as its to-do list,
/// with why it's as it is.
pub(super) async fn plan(shared: &Shared, params: &Value, scope: &Scope) -> Result<(), AgentError> {
    if scope.parent.is_some() {
        return Ok(());
    }
    let Plan { explanation, plan } = Plan::deserialize(params)?;
    let items = plan.into_iter().map(todo).collect();
    shared.emit(AgentOutput::Report(Report::Todos(TodoList { items, note: explanation }))).await;
    Ok(())
}

#[derive(Deserialize)]
struct Plan {
    explanation: Option<String>,
    plan: Vec<Step>,
}

#[derive(Deserialize)]
struct Step {
    step: String,
    status: StepStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum StepStatus {
    Pending,
    InProgress,
    Completed,
    #[serde(other)]
    Unknown,
}

fn todo(Step { step, status }: Step) -> Todo {
    let status = match status {
        StepStatus::Pending => TodoStatus::Pending,
        StepStatus::InProgress => TodoStatus::InProgress,
        StepStatus::Completed => TodoStatus::Done,
        StepStatus::Unknown => TodoStatus::Unknown,
    };
    Todo { id: None, text: step, status, doing: None, priority: None, after: Vec::new() }
}
