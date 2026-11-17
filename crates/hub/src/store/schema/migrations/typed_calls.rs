//! Typing what Claude's adapter recorded before it typed its calls, in place, as `crate::untyped` types it.

use rusqlite::{Transaction, params};
use serde_json::Value;

use crate::store::StoreError;
use crate::untyped::{Typing, lately};

/// How many events are read at a time, so a long log is typed a part at a time rather than held whole.
const AT_ONCE: i64 = 1000;

/// What a subagent's record kept in the older forms, which `lately` holds typed: its newest call, which only a client
/// from before `recent` read, and what it had done lately.
const OLDER_COLUMNS: &str = "ALTER TABLE subagents DROP COLUMN latest_call; ALTER TABLE subagents DROP COLUMN recent;";

/// Each call Claude's adapter recorded by its tool's name and input typed, with what came back for it, as each
/// session's events have them in order, the call each request for approval asked to make, and what a working
/// subagent's record says it has done lately, whose older columns go. Only a body that changes is written, and an
/// event the hub can't read is left as it is, as is a record whose work holds no call.
pub(super) fn typed_calls(tx: &Transaction<'_>) -> Result<(), StoreError> {
    events(tx)?;
    records(tx)?;
    Ok(tx.execute_batch(OLDER_COLUMNS)?)
}

fn events(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let mut read = tx.prepare(
        "SELECT session_id, seq, body FROM events
            WHERE (session_id, seq) > (?1, ?2) AND kind IN ('assistant', 'agent_user', 'approval_requested')
            ORDER BY session_id, seq LIMIT ?3",
    )?;
    let mut write = tx.prepare("UPDATE events SET body = ?3 WHERE session_id = ?1 AND seq = ?2")?;
    let (mut after, mut session, mut typing) = ((String::new(), i64::MIN), String::new(), Typing::default());
    loop {
        let rows = read
            .query_map(params![after.0, after.1, AT_ONCE], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let Some((last, seq, _)) = rows.last() else { return Ok(()) };
        after = (last.clone(), *seq);
        for (id, seq, body) in rows {
            if id != session {
                (session, typing) = (id.clone(), Typing::default());
            }
            let Ok(mut body) = serde_json::from_str::<Value>(&body) else { continue };
            if typing.body(&mut body) {
                write.execute(params![id, seq, serde_json::to_string(&body)?])?;
            }
        }
    }
}

fn records(tx: &Transaction<'_>) -> Result<(), StoreError> {
    let working = tx
        .prepare("SELECT session_id, call, recent FROM subagents WHERE recent IS NOT NULL")?
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut write = tx.prepare("UPDATE subagents SET lately = ?3 WHERE session_id = ?1 AND call = ?2")?;
    for (session, call, recent) in working {
        let Some(lately) = lately(&recent) else { continue };
        write.execute(params![session, call, serde_json::to_string(&lately)?])?;
    }
    Ok(())
}
