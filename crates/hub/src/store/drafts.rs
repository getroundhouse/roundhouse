//! What's typed and attached in each session's composer and not sent, and letting go of a file nothing will send.

use protocol::attachment::MAX_PER_PROMPT;
use protocol::model::{AttachmentId, Draft, NewDraft, SessionId};
use rusqlite::{Connection, params};

use super::attachments::read_attachment;
use super::reading::session_on;
use super::sessions::draft_column;
use super::{Reading, Store, StoreError, lock};

#[cfg(test)]
mod tests;

impl Store {
    /// Keeps `draft` as what's in the session's composer, in place of what was. Each file is named as the session
    /// has it, and one it hasn't been given is left out, as are any past what a prompt can carry. A closed session
    /// keeps none, since its composer went with it. Keeping what's kept already changes nothing, so the session's
    /// revision only moves when the draft does.
    pub fn set_draft(&self, id: &SessionId, draft: &NewDraft) -> Result<Reading, StoreError> {
        let conn = lock(&self.writer);
        let mut files = Vec::new();
        for file in draft.files.iter().take(MAX_PER_PROMPT) {
            files.extend(read_attachment(&conn, id, file)?);
        }
        let kept = Draft { text: draft.text.clone(), files };
        let changed = conn.execute(
            "UPDATE sessions SET draft = ?1 WHERE id = ?2 AND closed_at_ms IS NULL AND draft IS NOT ?1",
            params![draft_column(&kept)?, id.as_str()],
        )? > 0;
        Ok(Reading { session: session_on(&conn, id)?, changed })
    }

    /// Lets go of a file a client added, as [`let_go`] says. Says whether it did, so the caller lets go of its bytes
    /// too.
    pub fn let_go_of_attachment(&self, session_id: &SessionId, id: &AttachmentId) -> Result<bool, StoreError> {
        let_go(&lock(&self.writer), session_id, id)
    }
}

/// For a session being closed, inside the change that closes it: lets go of each file its draft named, as [`let_go`]
/// says, once the draft has gone with its composer. Returns the files let go of.
pub(super) fn forget_draft(conn: &Connection, id: &SessionId) -> Result<Vec<AttachmentId>, StoreError> {
    let mut named = conn.prepare(
        "SELECT json_extract(named.value, '$.id') FROM sessions, json_each(sessions.draft, '$.files') AS named
             WHERE sessions.id = ?",
    )?;
    let files: Vec<AttachmentId> = named
        .query_map([id.as_str()], |row| row.get::<_, String>(0))?
        .map(|file| file.map(Into::into))
        .collect::<Result<_, _>>()?;
    conn.execute("UPDATE sessions SET draft = NULL WHERE id = ?", [id.as_str()])?;
    let mut let_go_of = Vec::new();
    for file in files {
        if let_go(conn, id, &file)? {
            let_go_of.push(file);
        }
    }
    Ok(let_go_of)
}

/// Lets go of a file a client added that nothing will send: it isn't in the worktree, and neither the session's draft,
/// a prompt waiting in its queue, nor a prompt it was sent names it. Those are all that can name a client's file, so
/// the log's other events aren't read. An image from the agent is never let go of, and nor is a file from before the
/// hub knew who added it, since every one of those was a prompt's or an agent's.
fn let_go(conn: &Connection, session_id: &SessionId, id: &AttachmentId) -> Result<bool, StoreError> {
    let removed = conn.execute(
        "DELETE FROM attachments WHERE session_id = ?1 AND id = ?2 AND by_client AND placed_at IS NULL
             AND NOT EXISTS (
                 SELECT 1 FROM sessions, json_each(sessions.draft, '$.files') AS named
                 WHERE sessions.id = ?1 AND json_extract(named.value, '$.id') = ?2)
             AND NOT EXISTS (
                 SELECT 1 FROM sessions, json_each(sessions.queued) AS waiting,
                     json_each(waiting.value, '$.attachments') AS named
                 WHERE sessions.id = ?1 AND json_extract(named.value, '$.id') = ?2)
             AND NOT EXISTS (
                 SELECT 1 FROM events, json_each(events.body, '$.attachments') AS named
                 WHERE events.session_id = ?1 AND events.kind = 'user_prompt'
                     AND json_extract(named.value, '$.id') = ?2)",
        params![session_id.as_str(), id.as_str()],
    )?;
    Ok(removed > 0)
}
