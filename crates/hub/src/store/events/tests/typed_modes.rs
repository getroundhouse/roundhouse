//! What the migration that typed approval modes made of each chat's, each its log named, and the new-workspace
//! dialog's.

use protocol::capability::approvals::{ApprovalMode, Beyond, Sandbox, Unasked};
use protocol::event::EventBody;
use protocol::fixtures::{full_access, plan_mode};
use protocol::model::{Seq, SessionId};
use serde_json::{Value, json};

use crate::store::{Store, older, schema};

/// A database as the build before the migration left it, with a chat of each `(id, agent, mode)`, Claude's chat `s`
/// holding `bodies`, each an event's body as JSON, in order, and the dialog holding `dialog`.
fn migrated(dir: &std::path::Path, chats: &[(&str, &str, &str)], bodies: &[Value], dialog: &Value) -> Store {
    let quoted = |json: String| json.replace('\'', "''");
    let chats = chats.iter().map(|(id, agent, mode)| {
        format!(
            "INSERT INTO sessions (id, workspace_id, agent, model, permission_mode, effort, agent_session_id, status, \
             last_seq, created_at_ms, updated_at_ms) \
             VALUES ('{id}', 'w', '{agent}', 'opus', '{mode}', 'high', '{id}', 'idle', 9, 0, 0);"
        )
    });
    let events = (1..).zip(bodies).map(|(seq, body)| {
        format!(
            "INSERT INTO events (session_id, seq, ts_ms, kind, place, body) VALUES ('s', {seq}, 0, '{}', {seq}, '{}');",
            body["type"].as_str().unwrap(),
            quoted(body.to_string()),
        )
    });
    let dialog = format!("UPDATE new_workspace_draft SET draft = '{}';", quoted(dialog.to_string()));
    let rows: String = chats.chain(events).chain([dialog]).collect();
    older::store_from_before(dir, schema::BEFORE_TYPED_MODES, &rows)
}

/// Codex's plan mode, as its adapter claimed it as the migration was made, which plans in a read-only sandbox.
fn codexs_plan_mode() -> ApprovalMode {
    ApprovalMode { unasked: Unasked::Reads, beyond: Beyond::Asks, sandbox: Some(Sandbox::ReadOnly), plan: true }
}

#[test]
fn each_chats_mode_is_typed_by_its_kind() {
    let dir = tempfile::tempdir().unwrap();
    let chats = [
        ("s", "claude", "bypassPermissions"),
        ("planning", "claude", "plan"),
        ("codex", "codex", "plan"),
        ("agy", "antigravity", "bypassPermissions"),
        ("by_itself", "claude", "default"),
    ];

    let store = migrated(dir.path(), &chats, &[], &json!({}));

    let mode = |id: &str| store.get_session(&id.into()).unwrap().session.mode;
    assert_eq!((mode("s"), mode("planning")), (full_access(), plan_mode()));
    assert_eq!(mode("codex"), codexs_plan_mode());
    let refused = ApprovalMode { beyond: Beyond::Refused, ..full_access() };
    assert_eq!(mode("agy"), refused, "Antigravity's full access refuses what it would ask");
    assert_eq!(mode("by_itself"), ApprovalMode::UNKNOWN, "one an agent went into by itself, which had no name");
}

#[test]
fn each_mode_the_log_names_is_typed_by_its_chats_kind() {
    let dir = tempfile::tempdir().unwrap();
    let bodies = [
        json!({"type":"agent_init","agent_session_id":"a","model":"opus","cwd":"/w","permission_mode":"plan",
            "agent_version":"2.1"}),
        json!({"type":"agent_init","agent_session_id":"a","model":"opus","cwd":"/w","permission_mode":null}),
        json!({"type":"agent_mode","permission_mode":"bypassPermissions"}),
        json!({"type":"agent_mode","permission_mode":"acceptEdits"}),
    ];

    let store = migrated(dir.path(), &[("s", "claude", "bypassPermissions")], &bodies, &json!({}));

    let events = store.read_events(&SessionId::from("s"), Seq::new(1), 100, u64::MAX, false).unwrap();
    let [started, said_none, changed, by_itself] = &events.into_iter().map(|event| event.body).collect::<Vec<_>>()[..]
    else {
        panic!("not four events")
    };
    let EventBody::AgentInit { mode, agent_version, .. } = started else { panic!("{started:?}") };
    assert_eq!((*mode, agent_version.as_deref()), (Some(plan_mode()), Some("2.1")), "the rest as it was");
    assert!(matches!(said_none, EventBody::AgentInit { mode: None, .. }), "{said_none:?}");
    assert_eq!(*changed, EventBody::AgentMode { mode: full_access() });
    assert_eq!(*by_itself, EventBody::AgentMode { mode: ApprovalMode::UNKNOWN });
}

#[test]
fn the_dialogs_plan_mode_is_its_agents_or_else_claudes() {
    let cases = [
        (json!({"agent":"codex","text":"Fix it","plan":true}), Some(codexs_plan_mode())),
        (json!({"text":"Fix it","plan":true}), Some(plan_mode())),
        (json!({"agent":"codex","text":"Fix it"}), None),
    ];
    for (dialog, expected) in cases {
        let dir = tempfile::tempdir().unwrap();

        let store = migrated(dir.path(), &[("s", "claude", "bypassPermissions")], &[], &dialog);

        let kept = store.new_workspace_draft().unwrap();
        assert_eq!((kept.mode, kept.text.as_str()), (expected, "Fix it"), "{dialog}");
    }
}
