use super::super::{Store, StoreError, current_rev, from_text, json_at, lock, rev_of, text_of};
use protocol::event::SessionEvent;
use protocol::model::{ModelName, Rev, SessionId, Subagent, SubagentId, SubagentStatus, TouchedSubagents};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Row, ToSql, params};

const SUBAGENT_COLUMNS: &str = "session_id, call, kind, description, prompt, in_background, status, started_at_ms, \
     ended_at_ms, calls, report, rev, model, context_tokens, id, lately";

impl Store {
    /// The session's subagents, oldest first, read in one transaction with the revision they're as of.
    pub fn list_subagents(&self, session_id: &SessionId) -> Result<(Vec<Subagent>, Rev), StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let subagents = tx
            .prepare_cached(&format!(
                "SELECT {SUBAGENT_COLUMNS} FROM subagents WHERE session_id = ? ORDER BY started_at_ms, rowid"
            ))?
            .query_map([session_id.as_str()], subagent_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok((subagents, rev))
    }

    pub fn get_subagent(&self, session_id: &SessionId, subagent: &SubagentId) -> Result<Option<Subagent>, StoreError> {
        let conn = lock(&self.reader);
        let sql = format!("SELECT {SUBAGENT_COLUMNS} FROM subagents WHERE session_id = ? AND id = ?");
        Ok(conn.query_row(&sql, params![session_id.as_str(), subagent.as_str()], subagent_from_row).optional()?)
    }

    /// The subagent `named` names in `session_id`: the one with that ID, or else the one the call it names started,
    /// since a client from before subagents had IDs of their own names each by its call. One that names neither is
    /// taken as it is, which reads no events, as it did before.
    pub fn subagent_named(&self, session_id: &SessionId, named: &SubagentId) -> Result<SubagentId, StoreError> {
        Ok(self.recorded_subagent(session_id, named)?.unwrap_or_else(|| named.clone()))
    }

    /// The subagent `named` names in `session_id`, as [`Store::subagent_named`] finds it, if there's a record of it.
    pub fn recorded_subagent(
        &self,
        session_id: &SessionId,
        named: &SubagentId,
    ) -> Result<Option<SubagentId>, StoreError> {
        let conn = lock(&self.reader);
        let sql = "SELECT id FROM subagents WHERE session_id = ?1 AND (id = ?2 OR call = ?2) ORDER BY id = ?2 DESC";
        let found = conn.query_row(sql, params![session_id.as_str(), named.as_str()], |row| row.get::<_, String>(0));
        Ok(found.optional()?.map(SubagentId::from))
    }
}

/// Changes the records of the subagents `event` says something about, in the transaction that records it, as
/// `protocol`'s `Subagent::note` says, and says which. It reads only the records `Subagent::touched_by` says the event
/// can change, and none for an event that says nothing of a subagent, as most don't. It writes back only those the
/// rule changed, so one it leaves as it was keeps its revision and isn't sent again, and of each, only what the rule
/// changed, so what a newer build stored that this one can't read is kept as it is.
pub(in super::super) fn note_subagents(tx: &Connection, event: &SessionEvent) -> Result<Vec<SubagentId>, StoreError> {
    let Some(touched) = Subagent::touched_by(event) else { return Ok(Vec::new()) };
    let mut held = touched_records(tx, &event.session_id, &touched)?;
    let read = held.clone();
    let changed = Subagent::note(&mut held, event);
    for now in &changed {
        match read.iter().find(|was| was.id() == now.id()) {
            Some(was) => update(tx, was, now)?,
            None => insert(tx, now)?,
        }
    }
    Ok(changed.iter().map(|subagent| subagent.id()).collect())
}

/// The records in `session` that `touched` names.
fn touched_records(
    tx: &Connection,
    session: &SessionId,
    touched: &TouchedSubagents,
) -> Result<Vec<Subagent>, StoreError> {
    let read = |which: &str, values: &[&dyn ToSql]| -> rusqlite::Result<Vec<Subagent>> {
        tx.prepare_cached(&format!("SELECT {SUBAGENT_COLUMNS} FROM subagents WHERE session_id = ?1 AND {which}"))?
            .query_map(values, subagent_from_row)?
            .collect()
    };
    let session = session.as_str();
    Ok(match touched {
        TouchedSubagents::One(id) => read("id = ?2", params![session, id.as_str()])?,
        TouchedSubagents::Starting { subagent, call } => {
            read("(id = ?2 OR call = ?3)", params![session, subagent.as_str(), call.as_str()])?
        }
        TouchedSubagents::Working => read("status = ?2", params![session, text_of(&SubagentStatus::Working)])?,
    })
}

/// Records `subagent`, which a start made.
fn insert(tx: &Connection, subagent: &Subagent) -> Result<(), StoreError> {
    let sql = "INSERT INTO subagents
         (session_id, id, call, kind, description, prompt, in_background, status, started_at_ms, ended_at_ms, calls,
             report, model, context_tokens, lately)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    tx.prepare_cached(sql)?.execute(params![
        subagent.session_id.as_str(),
        subagent.id().as_str(),
        subagent.call.as_str(),
        subagent.kind,
        subagent.description,
        subagent.prompt,
        subagent.in_background,
        text_of(&subagent.status),
        subagent.started_at_ms,
        subagent.ended_at_ms,
        subagent.calls,
        subagent.report,
        subagent.model.as_ref().map(ModelName::as_str),
        subagent.context_tokens.map(u64::cast_signed),
        lately_column(subagent)?,
    ])?;
    Ok(())
}

// Each column the rule can change, set only when its flag says the rule changed it, so one this build read as
// something it doesn't know, or couldn't read, keeps what it holds. Which subagent it is, its call and what it was
// asked never change. Its revision is the triggers'.
const UPDATE: &str = "UPDATE subagents SET
         in_background = iif(?3, ?4, in_background),
         status = iif(?5, ?6, status),
         started_at_ms = iif(?7, ?8, started_at_ms),
         ended_at_ms = iif(?9, ?10, ended_at_ms),
         calls = iif(?11, ?12, calls),
         report = iif(?13, ?14, report),
         model = iif(?15, ?16, model),
         context_tokens = iif(?17, ?18, context_tokens),
         lately = iif(?19, ?20, lately)
     WHERE session_id = ?1 AND id = ?2";

/// Writes what the rule changed of the record that was `was` and is `now`.
fn update(tx: &Connection, was: &Subagent, now: &Subagent) -> Result<(), StoreError> {
    tx.prepare_cached(UPDATE)?.execute(params![
        now.session_id.as_str(),
        now.id().as_str(),
        was.in_background != now.in_background,
        now.in_background,
        was.status != now.status,
        text_of(&now.status),
        was.started_at_ms != now.started_at_ms,
        now.started_at_ms,
        was.ended_at_ms != now.ended_at_ms,
        now.ended_at_ms,
        was.calls != now.calls,
        now.calls,
        was.report != now.report,
        now.report,
        was.model != now.model,
        now.model.as_ref().map(ModelName::as_str),
        was.context_tokens != now.context_tokens,
        now.context_tokens.map(u64::cast_signed),
        was.lately != now.lately,
        lately_column(now)?,
    ])?;
    Ok(())
}

/// What `subagent` has done lately, as its column holds it.
fn lately_column(subagent: &Subagent) -> Result<Option<String>, StoreError> {
    Ok(subagent.lately.as_ref().map(serde_json::to_string).transpose()?)
}

fn subagent_from_row(row: &Row<'_>) -> rusqlite::Result<Subagent> {
    Ok(Subagent {
        session_id: row.get::<_, String>(0)?.into(),
        id: Some(row.get::<_, String>(14)?.into()),
        call: row.get::<_, String>(1)?.into(),
        kind: text_at(row, 2)?,
        description: text_at(row, 3)?,
        prompt: text_at(row, 4)?,
        in_background: row.get(5)?,
        // Written only by this hub, from a name that named a model. One that doesn't is none it knows of.
        model: row.get::<_, Option<String>>(12)?.and_then(|model| ModelName::try_from(model).ok()),
        status: from_text(row.get(6)?)?,
        started_at_ms: row.get(7)?,
        ended_at_ms: row.get(8)?,
        context_tokens: row.get::<_, Option<i64>>(13)?.map(i64::cast_unsigned),
        calls: row.get(9)?,
        report: text_at(row, 10)?,
        rev: rev_of(row, 11)?,
        // Written only by this hub, as JSON. What it can't read is nothing it knows of.
        lately: json_at(row, 15)?,
    })
}

/// What the CLI said, as the adapter would have kept it. The records filled in from logs from before them were read
/// from the CLI's own lines, where half of a surrogate pair, which JavaScript writes, comes out of SQLite as three
/// bytes that aren't UTF-8. The adapter repairs a line before reading it, making each half U+FFFD, and so does this,
/// so a record can't fail its session's list.
fn text_at(row: &Row<'_>, column: usize) -> rusqlite::Result<Option<String>> {
    let ValueRef::Text(mut bytes) = row.get_ref(column)? else { return row.get(column) };
    let mut text = String::with_capacity(bytes.len());
    loop {
        match std::str::from_utf8(bytes) {
            Ok(rest) => {
                text.push_str(rest);
                return Ok(Some(text));
            }
            Err(e) => {
                let (valid, rest) = bytes.split_at(e.valid_up_to());
                text.push_str(&String::from_utf8_lossy(valid));
                text.push(char::REPLACEMENT_CHARACTER);
                let half = matches!(rest, [0xED, 0xA0..=0xBF, 0x80..=0xBF, ..]);
                let skipped = if half { 3 } else { e.error_len().unwrap_or(rest.len()) };
                bytes = rest.get(skipped..).unwrap_or_default();
            }
        }
    }
}
