use super::super::{
    ByColumn, Columns, OneValue, Store, StoreError, columns, current_rev, lock, maybe_text_at, rev_of, text_of,
};
use protocol::model::{Ending, MessageId, ReplyHeard, Rev, SessionId, SettledBy};
use rusqlite::{Connection, OptionalExtension, Row, params};

columns! {
    /// The columns every read of a reply heard takes, as [`reply_heard_from_row`] reads them.
    struct ReplyHeardColumns { session_id, message, ending, rev, settled_by }
}

impl Store {
    /// Keeps how the reply `message` ended, as `by` said it, unless how it ended is kept already, since the first that's
    /// said of a reply is kept, but for what the hub settled, which what the device says takes the place of. A device
    /// that says none of it was read out can't undo how far it was heard, as one that took the conversation over
    /// partway through a reply says of it, though another device was reading it. Returns the record if it changed.
    pub fn keep_reply_heard(
        &self,
        session_id: &SessionId,
        message: &MessageId,
        ending: Ending,
        by: SettledBy,
    ) -> Result<Option<ReplyHeard>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let kept = tx.execute(
            "INSERT INTO replies_heard (session_id, message, ending, settled_by) VALUES (?1, ?2, ?3, ?4)
                ON CONFLICT DO UPDATE SET ending = excluded.ending, settled_by = excluded.settled_by
                WHERE replies_heard.settled_by = 'hub' AND ?4 IS NULL
                    AND NOT (?5 AND json_extract(replies_heard.ending, '$.through') IS NOT NULL)",
            params![
                session_id.as_str(),
                message.as_str(),
                serde_json::to_string(&ending)?,
                settled_by_column(by),
                ending == Ending::NotReadOut,
            ],
        )?;
        let heard = if kept == 0 { None } else { reply_heard_on(&tx, session_id, message)? };
        tx.commit()?;
        Ok(heard)
    }

    /// How the reply `message` ended, if that's kept.
    pub fn reply_heard(&self, session_id: &SessionId, message: &MessageId) -> Result<Option<ReplyHeard>, StoreError> {
        reply_heard_on(&lock(&self.reader), session_id, message)
    }

    /// The agent's own replies with something to read out, recorded in conversation mode, whose ending isn't kept, oldest
    /// first. A reply recorded in parts is one of them if any part was recorded in the mode and says something.
    pub fn replies_unheard(&self, session_id: &SessionId) -> Result<Vec<MessageId>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT r.message FROM (
                SELECT json_extract(e.body, '$.message_id') AS message, min(e.seq) AS first FROM events e
                WHERE e.session_id = ?1 AND e.kind = 'assistant' AND e.interaction = 'conversation'
                    AND e.subagent IS NULL
                    AND EXISTS (SELECT 1 FROM json_each(e.body, '$.blocks') WHERE json_extract(value, '$.type') = 'text')
                GROUP BY message
            ) r
            WHERE r.message IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM replies_heard h WHERE h.session_id = ?1 AND h.message = r.message)
            ORDER BY r.first",
        )?;
        let unheard = statement.query_map([session_id.as_str()], OneValue::read::<String>)?;
        Ok(unheard.map(|message| message.map(MessageId::from)).collect::<rusqlite::Result<_>>()?)
    }

    /// How each of the session's replies said out loud ended, read in one transaction with the revision they're as of.
    pub fn list_replies_heard(&self, session_id: &SessionId) -> Result<(Vec<ReplyHeard>, Rev), StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let heard = tx
            .prepare_cached(&format!(
                "SELECT {} FROM replies_heard WHERE session_id = ? ORDER BY rev",
                ReplyHeardColumns::LIST
            ))?
            .query_map([session_id.as_str()], reply_heard_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok((heard, rev))
    }
}

/// How the reply `message` ended, if that's kept, on `conn`.
fn reply_heard_on(
    conn: &Connection,
    session_id: &SessionId,
    message: &MessageId,
) -> Result<Option<ReplyHeard>, StoreError> {
    let heard = conn
        .query_row(
            &format!("SELECT {} FROM replies_heard WHERE session_id = ? AND message = ?", ReplyHeardColumns::LIST),
            params![session_id.as_str(), message.as_str()],
            reply_heard_from_row,
        )
        .optional()?;
    Ok(heard)
}

/// What the `settled_by` column holds for `by`: NULL for the device, as in every record from before.
fn settled_by_column(by: SettledBy) -> Option<String> {
    (!by.is_device()).then(|| text_of(&by))
}

/// Written only by this hub, as JSON. An ending it can't read is one it doesn't know.
fn reply_heard_from_row(row: &Row<'_>) -> rusqlite::Result<ReplyHeard> {
    let c = ReplyHeardColumns::at(0);
    Ok(ReplyHeard {
        session_id: SessionId::from(row.at::<String>(c.session_id)?),
        message: MessageId::from(row.at::<String>(c.message)?),
        ending: serde_json::from_str(&row.at::<String>(c.ending)?).unwrap_or(Ending::Unknown),
        settled_by: maybe_text_at(row, c.settled_by)?.unwrap_or(SettledBy::Device),
        rev: rev_of(row, c.rev)?,
    })
}
