//! The approval modes the hub and its workers named before modes were typed, as each kind's adapter claimed them then:
//! full access, which they called by Claude's CLI's name, `bypassPermissions`, and plan mode, `plan`. A mode an agent
//! went into by itself had no name, and has none here.

use protocol::capability::approvals::{ApprovalMode, Beyond, Sandbox, Unasked};
use protocol::model::AgentKind;
use serde_json::Value;

/// The mode a chat of `kind` was in where it was named `name`.
pub(crate) fn mode(kind: AgentKind, name: &str) -> ApprovalMode {
    let full_access = |beyond| ApprovalMode { unasked: Unasked::Everything, beyond, sandbox: None, plan: false };
    let planning = |sandbox| ApprovalMode { unasked: Unasked::Reads, beyond: Beyond::Asks, sandbox, plan: true };
    match (kind, name) {
        (AgentKind::Claude, "bypassPermissions") => full_access(Beyond::Asks),
        (AgentKind::Codex | AgentKind::Antigravity, "bypassPermissions") => full_access(Beyond::Refused),
        (AgentKind::Claude | AgentKind::Antigravity, "plan") => planning(None),
        (AgentKind::Codex, "plan") => planning(Some(Sandbox::ReadOnly)),
        _ => ApprovalMode::UNKNOWN,
    }
}

/// Types the mode `body`, an event's body as JSON from a chat of `kind`, names in the older form, in place, and says
/// whether it named one: what an agent reported changing to, or starting in.
pub(crate) fn type_mode(body: &mut Value, kind: AgentKind) -> bool {
    let reports = matches!(body.get("type").and_then(Value::as_str), Some("agent_mode" | "agent_init"));
    let Some(fields) = body.as_object_mut().filter(|_| reports) else { return false };
    let Some(named) = fields.remove("permission_mode") else { return false };
    if let Some(name) = named.as_str()
        && let Ok(typed) = serde_json::to_value(mode(kind, name))
    {
        fields.insert("mode".into(), typed);
    }
    true
}
