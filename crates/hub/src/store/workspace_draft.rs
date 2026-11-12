//! What's in the new-workspace dialog and not made yet, which is one record, and the files waiting with it. No session
//! exists yet to add them to, so they're kept apart from every session's. What a file is never changes once it's added.
//! Its bytes aren't here, but in a file of their own, which `attachment_files` keeps.

use protocol::attachment::{Attachment, MAX_PER_PROMPT};
use protocol::model::{AttachmentId, Rev, WorkspaceDraft, WorkspaceDraftToKeep};
use rusqlite::{Connection, OptionalExtension as _, params};

use super::{Store, StoreError, from_text, lock, now_ms, rev_of, text_of};

#[cfg(test)]
mod tests;

pub(super) fn workspace_draft(conn: &Connection) -> rusqlite::Result<WorkspaceDraft> {
    let (draft, rev) = conn.query_row("SELECT draft, rev FROM workspace_draft", [], |row| {
        Ok((row.get::<_, Option<String>>(0)?, rev_of(row, 1)?))
    })?;
    // Written only by this hub, as JSON. One it can't read is as good as none.
    let draft: WorkspaceDraft = draft.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default();
    Ok(WorkspaceDraft { rev, ..draft })
}

impl Store {
    pub fn workspace_draft(&self) -> Result<WorkspaceDraft, StoreError> {
        Ok(workspace_draft(&lock(&self.reader))?)
    }

    /// Keeps `draft` as what's in the new-workspace dialog, in place of what was. Each file is named as the hub has it,
    /// and one it hasn't been given is left out, as are any past what a prompt can carry. Keeping what's kept already
    /// changes nothing, so the record's revision only moves when the draft does. Returns the draft, and whether it
    /// changed.
    pub fn set_workspace_draft(&self, draft: &WorkspaceDraftToKeep) -> Result<(WorkspaceDraft, bool), StoreError> {
        let conn = lock(&self.writer);
        let mut files = Vec::new();
        for file in draft.files.iter().take(MAX_PER_PROMPT) {
            files.extend(read_file(&conn, file)?);
        }
        let WorkspaceDraftToKeep { repo_id, text, model, effort, plan, .. } = draft.clone();
        // Without its revision, which is the row's, so the same draft is always the same JSON.
        let kept = WorkspaceDraft { repo_id, text, files, model, effort, plan, rev: Rev::default() };
        let changed = conn.execute(
            "UPDATE workspace_draft SET draft = ?1 WHERE draft IS NOT ?1",
            params![serde_json::to_string(&kept)?],
        )? > 0;
        Ok((workspace_draft(&conn)?, changed))
    }

    /// Records `file` for the draft to name, unless one with its ID is there already, and returns the one recorded, so
    /// adding the same ID again returns the first.
    pub fn add_workspace_draft_file(&self, file: &Attachment) -> Result<Attachment, StoreError> {
        let conn = lock(&self.writer);
        conn.execute(
            "INSERT INTO workspace_draft_files (id, name, media_type, size_bytes, created_at_ms)
             VALUES (?, ?, ?, ?, ?) ON CONFLICT (id) DO NOTHING",
            params![file.id.as_str(), file.name, text_of(&file.media_type), file.size_bytes.cast_signed(), now_ms()],
        )?;
        read_file(&conn, &file.id)?.ok_or_else(|| not_found(&file.id))
    }

    /// What `id` is, if the hub has it for the draft.
    pub fn workspace_draft_file(&self, id: &AttachmentId) -> Result<Option<Attachment>, StoreError> {
        read_file(&lock(&self.reader), id)
    }

    /// Lets go of a file the draft doesn't name. Says whether it did, so the caller lets go of its bytes too.
    pub fn let_go_of_workspace_draft_file(&self, id: &AttachmentId) -> Result<bool, StoreError> {
        let removed = lock(&self.writer).execute(
            "DELETE FROM workspace_draft_files WHERE id = ?1 AND NOT EXISTS (
                 SELECT 1 FROM workspace_draft, json_each(workspace_draft.draft, '$.files') AS named
                 WHERE json_extract(named.value, '$.id') = ?1)",
            [id.as_str()],
        )?;
        Ok(removed > 0)
    }
}

fn read_file(conn: &Connection, id: &AttachmentId) -> Result<Option<Attachment>, StoreError> {
    let file = conn
        .query_row(
            "SELECT name, media_type, size_bytes FROM workspace_draft_files WHERE id = ?",
            [id.as_str()],
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
    Ok(file)
}

fn not_found(id: &AttachmentId) -> StoreError {
    StoreError::NotFound { kind: "attachment", id: id.to_string() }
}
