use protocol::model::ToolUseId;
use serde_json::Value;

use crate::codex::{
    running::Shared,
    state::{ThreadId, TurnId},
};

#[derive(Default)]
pub(in crate::codex) struct Scope {
    pub(in crate::codex) parent: Option<ToolUseId>,
    pub(in crate::codex) model: Option<String>,
}

/// No thread means a connection-wide notification. A foreign thread is never implicitly the root, even before its
/// activity item has supplied the association; callers retain such a notification as an untranslated diagnostic.
pub(in crate::codex) fn scope(shared: &Shared, params: &Value) -> Option<Scope> {
    let Some(thread) = params["threadId"].as_str() else { return Some(Scope::default()) };
    let state = shared.state.lock();
    if state.thread.as_ref().is_none_or(|root| root.0 == thread) {
        return Some(Scope::default());
    }
    let child = state.subagents.threads.get(&ThreadId(thread.into()))?;
    let turn = params["turnId"].as_str().or_else(|| params["turn"]["id"].as_str());
    let call = match turn {
        Some(turn) => child.turns.get(&TurnId(turn.into()))?,
        None => &child.current,
    };
    let run = state.subagents.runs.get(call)?;
    Some(Scope { parent: Some(call.clone()), model: run.details.model.as_ref().map(|model| model.as_str().to_owned()) })
}
