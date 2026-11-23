use super::super::new_workspace_draft::read_draft_file;
use super::super::{ByColumn, Columns, OneValue, Store, StoreError, columns, lock, now_ms, text_at, text_of};
use crate::attachment_files::FileFor;
use protocol::attachment::{Attachment, MAX_PER_PROMPT};
use protocol::model::{AttachmentId, SessionId, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, Row, params};

/// Who added a file to a session. Only a client's can be in a draft, so only those can a client have the hub let go of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddedBy {
    Client,
    /// An image a tool sent, or one the agent's text points to.
    Agent,
}

impl Store {
    /// Records `attachment` in the session, unless the session already has one with its ID, and returns the one
    /// recorded, so adding the same ID again returns the first, as whoever added it first.
    pub fn add_attachment(
        &self,
        session_id: &SessionId,
        attachment: &Attachment,
        by: AddedBy,
    ) -> Result<Attachment, StoreError> {
        let conn = lock(&self.writer);
        // Under the writer's lock, so a session deleted meanwhile is found missing rather than failing the insert.
        let session: Option<i64> =
            conn.query_row("SELECT 1 FROM sessions WHERE id = ?", [session_id.as_str()], OneValue::read).optional()?;
        if session.is_none() {
            return Err(StoreError::NotFound { kind: "session", id: session_id.to_string() });
        }
        conn.execute(
            "INSERT INTO attachments (session_id, id, name, media_type, size_bytes, created_at_ms, by_client)
             VALUES (?, ?, ?, ?, ?, ?, ?) ON CONFLICT (session_id, id) DO NOTHING",
            params![
                session_id.as_str(),
                attachment.id.as_str(),
                attachment.name,
                text_of(&attachment.media_type),
                attachment.size_bytes.cast_signed(),
                now_ms(),
                by == AddedBy::Client,
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
                OneValue::read,
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

    /// The images made of pixels the prompts of the workspace's chats carried, oldest first whichever chat each was in,
    /// as [`Store::workspace_prompt_lines`] lists the prompts, and in the order each prompt carried them.
    pub fn workspace_prompt_images(
        &self,
        workspace_id: &WorkspaceId,
    ) -> Result<Vec<(SessionId, Attachment)>, StoreError> {
        let conn = lock(&self.reader);
        columns! {
            struct ImageColumns { session_id = "e.session_id", attachment = "a.value" }
        }
        let c = ImageColumns::at(0);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM sessions s \
             JOIN events e ON e.session_id = s.id AND e.kind = 'user_prompt' \
             JOIN json_each(e.body, '$.attachments') a \
             WHERE s.workspace_id = ? ORDER BY e.ts_ms, s.created_at_ms, e.seq, a.key",
            ImageColumns::LIST
        ))?;
        let mut rows = statement.query([workspace_id.as_str()])?;
        let mut images = Vec::new();
        while let Some(row) = rows.next()? {
            let attachment: Attachment = serde_json::from_str(&row.at::<String>(c.attachment)?)?;
            if attachment.media_type.is_bitmap() {
                images.push((SessionId::from(row.at::<String>(c.session_id)?), attachment));
            }
        }
        Ok(images)
    }

    /// What each of `ids` is, in the same order. Fails on the first the session doesn't have.
    pub fn attachments(&self, session_id: &SessionId, ids: &[AttachmentId]) -> Result<Vec<Attachment>, StoreError> {
        let conn = lock(&self.reader);
        ids.iter().map(|id| read_attachment(&conn, session_id, id)?.ok_or_else(|| not_found(id))).collect()
    }

    /// What `id` is, if the hub has it for what it's kept for.
    pub(crate) fn file(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<Option<Attachment>, StoreError> {
        file_on(&lock(&self.reader), kept_for, id)
    }

    /// Whether the agent added `id` to what it's kept for, as an image or a video its text points to, rather than a
    /// client. Only a session's can be the agent's.
    pub(crate) fn added_by_agent(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<bool, StoreError> {
        let FileFor::Session(session_id) = kept_for else { return Ok(false) };
        let by_client: Option<bool> = lock(&self.reader)
            .query_row(
                "SELECT by_client FROM attachments WHERE session_id = ? AND id = ?",
                params![session_id.as_str(), id.as_str()],
                OneValue::read,
            )
            .optional()?;
        Ok(by_client == Some(false))
    }

    /// Records `file` for what it's kept for, as a session's or the new-workspace dialog's, whose files only a client
    /// adds, and returns the one recorded, so adding the same ID again returns the first.
    pub(crate) fn add_file(
        &self,
        kept_for: &FileFor,
        file: &Attachment,
        by: AddedBy,
    ) -> Result<Attachment, StoreError> {
        match kept_for {
            FileFor::Session(session_id) => self.add_attachment(session_id, file, by),
            FileFor::NewWorkspace => self.add_new_workspace_draft_file(file),
        }
    }

    /// Lets go of a file a client added that nothing will send, as a session's or the new-workspace dialog's. Says
    /// whether it did, so the caller lets go of its bytes too.
    pub(crate) fn let_go_of_file(&self, kept_for: &FileFor, id: &AttachmentId) -> Result<bool, StoreError> {
        match kept_for {
            FileFor::Session(session_id) => self.let_go_of_attachment(session_id, id),
            FileFor::NewWorkspace => self.let_go_of_new_workspace_draft_file(id),
        }
    }
}

fn file_on(conn: &Connection, kept_for: &FileFor, id: &AttachmentId) -> Result<Option<Attachment>, StoreError> {
    match kept_for {
        FileFor::Session(session_id) => read_attachment(conn, session_id, id),
        FileFor::NewWorkspace => read_draft_file(conn, id),
    }
}

/// The files a draft names, as the hub has them for what they're kept for. One it hasn't been given is left out, as
/// are any past what a prompt can carry.
pub(in super::super) fn named_on(
    conn: &Connection,
    kept_for: &FileFor,
    ids: &[AttachmentId],
) -> Result<Vec<Attachment>, StoreError> {
    let mut files = Vec::new();
    for id in ids.iter().take(MAX_PER_PROMPT) {
        files.extend(file_on(conn, kept_for, id)?);
    }
    Ok(files)
}

columns! {
    /// What a file added to a composer holds, in the files of a chat's or of the new workspace's, as
    /// [`file_from_row`] reads it.
    pub(in super::super) struct FileColumns { name, media_type, size_bytes }
}

/// The file `id` in a row of [`FileColumns`].
pub(in super::super) fn file_from_row(row: &Row<'_>, id: &AttachmentId) -> rusqlite::Result<Attachment> {
    let c = FileColumns::at(0);
    Ok(Attachment {
        id: id.clone(),
        name: row.at(c.name)?,
        media_type: text_at(row, c.media_type)?,
        size_bytes: row.at::<i64>(c.size_bytes)?.cast_unsigned(),
    })
}

pub(in super::super) fn read_attachment(
    conn: &Connection,
    session_id: &SessionId,
    id: &AttachmentId,
) -> Result<Option<Attachment>, StoreError> {
    let attachment = conn
        .query_row(
            &format!("SELECT {} FROM attachments WHERE session_id = ? AND id = ?", FileColumns::LIST),
            params![session_id.as_str(), id.as_str()],
            |row| file_from_row(row, id),
        )
        .optional()?;
    Ok(attachment)
}

fn not_found(id: &AttachmentId) -> StoreError {
    StoreError::NotFound { kind: "attachment", id: id.to_string() }
}
