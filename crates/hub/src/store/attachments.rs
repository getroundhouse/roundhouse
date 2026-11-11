//! What the files a session's prompts carry are, and the images its agent's tools send, and where in the worktree each
//! of a prompt's files was put. What a file is never changes once it's added. Its bytes aren't here, but in a file of
//! their own, which `attachment_files` keeps.

use protocol::attachment::Attachment;
use protocol::model::{AttachmentId, SessionId, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, params};

use super::{Store, StoreError, from_text, lock, now_ms, text_of};

impl Store {
    /// Records `attachment` in the session, unless the session already has one with its ID, and returns the one
    /// recorded, so adding the same ID again returns the first.
    pub fn add_attachment(&self, session_id: &SessionId, attachment: &Attachment) -> Result<Attachment, StoreError> {
        let conn = lock(&self.writer);
        // Under the writer's lock, so a session deleted meanwhile is found missing rather than failing the insert.
        let session: Option<i64> = conn
            .query_row("SELECT 1 FROM sessions WHERE id = ?", [session_id.as_str()], |row| row.get(0))
            .optional()?;
        if session.is_none() {
            return Err(StoreError::NotFound { kind: "session", id: session_id.to_string() });
        }
        conn.execute(
            "INSERT INTO attachments (session_id, id, name, media_type, size_bytes, created_at_ms)
             VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT (session_id, id) DO NOTHING",
            params![
                session_id.as_str(),
                attachment.id.as_str(),
                attachment.name,
                text_of(&attachment.media_type),
                attachment.size_bytes.cast_signed(),
                now_ms(),
            ],
        )?;
        read_attachment(&conn, session_id, &attachment.id)?.ok_or_else(|| not_found(&attachment.id))
    }

    /// What `id` is, if the session has it.
    pub fn attachment(&self, session_id: &SessionId, id: &AttachmentId) -> Result<Option<Attachment>, StoreError> {
        read_attachment(&lock(&self.reader), session_id, id)
    }

    /// Where the worker put `id` in the worktree, if a prompt has carried it there.
    pub fn attachment_placed_at(
        &self,
        session_id: &SessionId,
        id: &AttachmentId,
    ) -> Result<Option<String>, StoreError> {
        let placed: Option<Option<String>> = lock(&self.reader)
            .query_row(
                "SELECT placed_at FROM attachments WHERE session_id = ? AND id = ?",
                params![session_id.as_str(), id.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        placed.ok_or_else(|| not_found(id))
    }

    pub fn set_attachment_placed_at(
        &self,
        session_id: &SessionId,
        id: &AttachmentId,
        path: &str,
    ) -> Result<(), StoreError> {
        let changed = lock(&self.writer).execute(
            "UPDATE attachments SET placed_at = ? WHERE session_id = ? AND id = ?",
            params![path, session_id.as_str(), id.as_str()],
        )?;
        if changed == 0 { Err(not_found(id)) } else { Ok(()) }
    }

    /// The images the prompts of the workspace's chats carried, oldest first whichever chat each was in, as
    /// [`Store::workspace_prompt_lines`] lists the prompts, and in the order each prompt carried them.
    pub fn workspace_prompt_images(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<(SessionId, Attachment)>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(
            "SELECT e.session_id, a.value FROM sessions s \
             JOIN events e ON e.session_id = s.id AND e.kind = 'user_prompt' \
             JOIN json_each(e.body, '$.attachments') a \
             WHERE s.workspace_id = ? ORDER BY e.ts_ms, s.created_at_ms, e.seq, a.key",
        )?;
        let mut rows = statement.query([workspace_id.as_str()])?;
        let mut images = Vec::new();
        while let Some(row) = rows.next()? {
            let attachment: Attachment = serde_json::from_str(&row.get::<_, String>(1)?)?;
            if attachment.media_type.is_image() {
                images.push((SessionId::from(row.get::<_, String>(0)?), attachment));
            }
        }
        Ok(images)
    }

    /// What each of `ids` is, in the same order. Fails on the first the session doesn't have.
    pub fn attachments(&self, session_id: &SessionId, ids: &[AttachmentId]) -> Result<Vec<Attachment>, StoreError> {
        let conn = lock(&self.reader);
        ids.iter().map(|id| read_attachment(&conn, session_id, id)?.ok_or_else(|| not_found(id))).collect()
    }
}

fn read_attachment(
    conn: &Connection,
    session_id: &SessionId,
    id: &AttachmentId,
) -> Result<Option<Attachment>, StoreError> {
    let attachment = conn
        .query_row(
            "SELECT name, media_type, size_bytes FROM attachments WHERE session_id = ? AND id = ?",
            params![session_id.as_str(), id.as_str()],
            |row| {
                Ok(Attachment {
                    id: id.clone(),
                    name: row.get(0)?,
                    media_type: from_text(row.get(1)?)?,
                    size_bytes: row.get::<_, i64>(2)?.cast_unsigned(),
                })
            },
        )
        .optional()?;
    Ok(attachment)
}

fn not_found(id: &AttachmentId) -> StoreError {
    StoreError::NotFound { kind: "attachment", id: id.to_string() }
}

#[cfg(test)]
mod tests;
