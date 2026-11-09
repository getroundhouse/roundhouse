//! What the files a session's prompts carry are, and where in the worktree each was put. What a file is never changes
//! once it's added. Its bytes aren't here, but in a file of their own, which `attachment_files` keeps.

use protocol::attachment::Attachment;
use protocol::model::{AttachmentId, SessionId};
use rusqlite::{Connection, OptionalExtension, params};

use super::{Store, StoreError, from_text, lock, now_ms, text_of};

impl Store {
    /// Records `attachment` in the session, unless the session already has one with its ID, and returns the one
    /// recorded, so adding the same ID again returns the first.
    pub fn add_attachment(&self, session_id: &SessionId, attachment: &Attachment) -> Result<Attachment, StoreError> {
        let conn = lock(&self.writer);
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
