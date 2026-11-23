//! How full a thread's context is, and how much of the account's limits is used, as Codex says while it works.

use protocol::capability::Report;
use protocol::capability::account::Limit;
use protocol::event::EventBody;
use protocol::model::{ContextUsage, LimitWindow, UsageLimit};
use serde_json::Value;

use super::{running::Shared, subagents::Scope};
use crate::AgentOutput;

/// `thread/tokenUsage/updated`: what the thread's newest request to the model held, which is what its context holds
/// now, as Codex's own interface counts it. The agent's own goes beside the log, since only the newest matters. A
/// subagent's is an event of the agent's, about it, as its record keeps the newest. One that doesn't say how much the
/// context holds says nothing to keep.
pub(super) async fn tokens(shared: &Shared, params: &Value, scope: &Scope, raw: String) {
    let usage = &params["tokenUsage"];
    let Some(used_tokens) = usage["last"]["totalTokens"].as_u64() else { return };
    if let Some(subagent) = &scope.parent {
        let body = EventBody::SubagentContext { subagent: subagent.clone(), tokens: used_tokens };
        shared.run.event(body, Some(raw)).await;
        return;
    }
    let Some(max_tokens) = usage["modelContextWindow"].as_u64() else { return };
    // Codex's window is its configuration's, which can be smaller than its model's, and it doesn't say which.
    let context = ContextUsage { used_tokens, max_tokens, model_tokens: None, max_set_by: None, parts: Vec::new() };
    shared.run.emit(AgentOutput::Report(Report::Context { context })).await;
}

/// `account/rateLimits/updated`: the account's limits, each window it names, which go to the hub as a report, outside
/// the log, since they're the account's and the hub keeps only the newest. An update can leave out what hasn't
/// changed, which the hub keeps. One whose windows have no name here is kept as it came, like any line with no form of
/// its own.
pub(super) async fn limits(shared: &Shared, params: &Value, scope: &Scope, raw: String) {
    let limits = limits_of(&params["rateLimits"]);
    if limits.is_empty() {
        let body = EventBody::AgentOther { kind: "account/rateLimits/updated".into(), subagent: scope.parent.clone() };
        shared.run.event(body, Some(raw)).await;
        return;
    }
    let limits = limits.iter().map(Limit::of).collect();
    shared.run.emit(AgentOutput::Report(Report::Limits { limits, every_models_week: false })).await;
}

/// The windows of the account's limit on every model, `codex`, shortest first. Another ID is a model's own limit, which
/// names no model a person would know it by, so it's left out.
pub(super) fn limits_of(snapshot: &Value) -> Vec<UsageLimit> {
    if snapshot["limitId"].as_str().is_some_and(|id| id != "codex") {
        return Vec::new();
    }
    let mut limits: Vec<_> = [&snapshot["primary"], &snapshot["secondary"]].into_iter().filter_map(window).collect();
    limits.sort_by_key(|limit| limit.window);
    limits
}

fn window(window: &Value) -> Option<UsageLimit> {
    let kind = match window["windowDurationMins"].as_u64()? {
        300 => LimitWindow::FiveHours,
        10_080 => LimitWindow::SevenDays,
        _ => return None,
    };
    let used = window["usedPercent"].as_u64()?;
    Some(UsageLimit {
        window: kind,
        model: None,
        // More than all of it reads as all of it.
        used_percent: u8::try_from(used.min(100)).unwrap_or(100),
        resets_at_ms: window["resetsAt"].as_i64().map(|seconds| seconds.saturating_mul(1000)),
        heard_at_ms: None,
    })
}
