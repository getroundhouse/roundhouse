//! Typing each chat's approval mode, each one its log names, and the new-workspace dialog's, by the chat's kind, as
//! `crate::untyped` types them.

use protocol::model::AgentKind;
use rusqlite::{Transaction, params};
use serde_json::Value;

use crate::store::StoreError;
use crate::untyped::{mode, type_mode};

/// Each chat's mode typed, in the column `mode` in place of `permission_mode`, each its agent reported or the hub
/// recorded in its log, and the one picked in the dialog, which said only whether its chat would plan. Only a body that
/// changes is written, and an event the hub can't read is left as it is.
pub(super) fn typed_modes(tx: &Transaction<'_>) -> Result<(), StoreError> {
    sessions(tx)?;
    events(tx)?;
    dialog(tx)
}

/// The kind `agent`, as the store keeps it, which is unknown for one this build doesn't run.
fn kind(agent: &str) -> AgentKind {
    AgentKind::parse(agent).unwrap_or(AgentKind::Unknown)
}

/// Typing a chat's mode moves no record's revision, since a client of a build from before can't read the typed form,
/// and one of this build never held the older one, so the trigger that would is set aside meanwhile, and put back as
/// renaming the column left it.
fn sessions(tx: &Transaction<'_>) -> Result<(), StoreError> {
    tx.execute_batch("ALTER TABLE sessions RENAME COLUMN permission_mode TO mode")?;
    let named = "SELECT sql FROM sqlite_master WHERE type = 'trigger' AND name = 'sessions_rev_on_update'";
    let revisions: String = tx.query_row(named, [], |row| row.get(0))?;
    tx.execute_batch("DROP TRIGGER sessions_rev_on_update")?;
    let rows = tx
        .prepare("SELECT id, agent, mode FROM sessions")?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, Option<String>>(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE sessions SET mode = ?2 WHERE id = ?1")?;
    for (id, agent, named) in rows {
        // One that names none has full access, as a chat starts, as the migration that kept only plan mode had it.
        let named = named.as_deref().unwrap_or("bypassPermissions");
        write.execute(params![id, serde_json::to_string(&mode(kind(&agent), named))?])?;
    }
    tx.execute_batch(&revisions)?;
    Ok(())
}

fn events(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let rows = tx
        .prepare(
            "SELECT events.session_id, events.seq, events.body, sessions.agent FROM events
                JOIN sessions ON sessions.id = events.session_id
                WHERE events.kind IN ('agent_mode', 'agent_init') AND instr(events.body, '\"permission_mode\"') > 0",
        )?
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE events SET body = ?3 WHERE session_id = ?1 AND seq = ?2")?;
    for (session, seq, body, agent) in rows {
        let Ok(mut body) = serde_json::from_str::<Value>(&body) else { continue };
        if type_mode(&mut body, kind(&agent)) {
            write.execute(params![session, seq, serde_json::to_string(&body)?])?;
        }
    }
    Ok(())
}

/// The dialog's mode, by the kind of agent it names, or else Claude's, which is the first kind the hub describes, so the
/// one a chat that names none is made with.
fn dialog(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let drafts = tx
        .prepare("SELECT rowid, draft FROM new_workspace_draft WHERE draft IS NOT NULL")?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE new_workspace_draft SET draft = ?2 WHERE rowid = ?1")?;
    for (row, draft) in drafts {
        let Ok(Value::Object(mut draft)) = serde_json::from_str::<Value>(&draft) else { continue };
        let Some(plans) = draft.remove("plan") else { continue };
        if plans == Value::Bool(true) {
            let agent = draft.get("agent").and_then(Value::as_str).map_or(AgentKind::Claude, kind);
            draft.insert("mode".into(), serde_json::to_value(mode(agent, "plan"))?);
        }
        write.execute(params![row, serde_json::to_string(&draft)?])?;
    }
    Ok(())
}
