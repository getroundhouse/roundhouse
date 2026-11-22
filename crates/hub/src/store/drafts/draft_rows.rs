use super::super::attachments::named_on;
use super::super::reading::session_on;
use super::super::sessions::draft_column;
use super::super::workspaces::workspace_on;
use super::super::{OneValue, Reading, Store, StoreError, lock, now_ms};
use crate::attachment_files::FileFor;
use protocol::model::{
    AttachmentId, CommentDraft, DecisionDraft, Draft, DraftToKeep, SessionId, Workspace, WorkspaceId,
};
use rusqlite::{Connection, OptionalExtension as _, params};

impl Store {
    /// Keeps `draft` as what's in the session's composer, in place of what was, unless what was is from a change typed
    /// after it, as `SessionDraft` says of `typed_at_ms`, when it's refused. Each file is named as the session has it,
    /// and one it hasn't been given is left out, as are any past what a prompt can carry. A closed session keeps none,
    /// since its composer went with it. Keeping what's kept already changes nothing, so the session's revision only
    /// moves when the draft does, and nor is it refused, however long ago it was typed.
    ///
    /// It lets go of no file, even one the draft stops naming. A client empties the draft as it sends a prompt, and
    /// the prompt that carries the draft's files may not be recorded yet. So a client asks for each file to be let go
    /// of, with `AttachmentRemove`, and one it never asks about, such as when it stops first, stays with the session.
    pub fn set_draft(
        &self,
        id: &SessionId,
        draft: &DraftToKeep,
        typed_at_ms: Option<i64>,
    ) -> Result<Reading, StoreError> {
        let conn = lock(&self.writer);
        let files = named_on(&conn, &FileFor::Session(id.clone()), &draft.files)?;
        let kept = draft_column(&Draft { text: draft.text.clone(), files })?;
        let now = now_ms();
        let typed = typed_at_ms.map_or(now, |at| at.min(now));
        // Only a change that says when it was typed can be typed earlier. One from an older client is typed as it comes.
        if typed_at_ms.is_some() {
            let typed_later = conn.query_row(
                "SELECT draft_typed_at_ms > ?3 AND draft IS NOT ?1 FROM sessions WHERE id = ?2 AND closed_at_ms IS NULL",
                params![kept, id.as_str(), typed],
                OneValue::read::<Option<bool>>,
            );
            if typed_later.optional()?.flatten() == Some(true) {
                return Err(StoreError::TypedEarlier);
            }
        }
        let changed = conn.execute(
            "UPDATE sessions SET draft = ?1, draft_typed_at_ms = ?3
                 WHERE id = ?2 AND closed_at_ms IS NULL AND draft IS NOT ?1",
            params![kept, id.as_str(), typed],
        )? > 0;
        if !changed {
            // The same again, typed later, such as on another device. Only when it was typed moves, which no record
            // shows, so the revision stays.
            conn.execute(
                "UPDATE sessions SET draft_typed_at_ms = ?2 WHERE id = ?1 AND closed_at_ms IS NULL
                     AND (draft_typed_at_ms IS NULL OR draft_typed_at_ms < ?2)",
                params![id.as_str(), typed],
            )?;
        }
        Ok(Reading { session: session_on(&conn, id)?, changed })
    }

    /// Keeps `draft` as what's decided so far on the request the session's agent is waiting on, in place of what was,
    /// or clears it with `None`, as one that says nothing does. What's typed in answer to a question whose answer is
    /// secret isn't kept, as `DecisionDraft::without_secrets` says. A closed session keeps none, since its request went
    /// with it, and nor does a request that's been decided or cancelled, as one typed in another client as it was can
    /// arrive after. Keeping what's kept already changes nothing, so the session's revision only moves when the draft
    /// does.
    pub fn set_decision_draft(&self, id: &SessionId, draft: Option<&DecisionDraft>) -> Result<Reading, StoreError> {
        let conn = lock(&self.writer);
        let mut draft = draft.cloned();
        if let Some(kept) = &draft
            && let Some(request) = super::super::approvals::request_on(&conn, id, &kept.approval_id)?
        {
            draft = draft.map(|draft| draft.without_secrets(&request));
        }
        let draft = draft.as_ref().filter(|draft| draft.says_anything());
        let json = draft.map(serde_json::to_string).transpose()?;
        let changed = conn.execute(
            "UPDATE sessions SET decision_draft = ?1 WHERE id = ?2 AND closed_at_ms IS NULL AND decision_draft IS NOT ?1
                 AND NOT EXISTS (
                     SELECT 1 FROM events WHERE session_id = ?2 AND kind = 'approval_resolved'
                         AND json_extract(body, '$.approval_id') = ?3)",
            params![json, id.as_str(), draft.map(|draft| draft.approval_id.as_str())],
        )? > 0;
        Ok(Reading { session: session_on(&conn, id)?, changed })
    }

    /// Keeps `draft` as the comment being written on the workspace's diff, in place of what was, or clears it with
    /// `None`. Keeping what's kept already changes nothing, so the workspace's revision only moves when the draft does.
    /// Returns the workspace, and whether it changed.
    pub fn set_comment_draft(
        &self,
        id: &WorkspaceId,
        draft: Option<&CommentDraft>,
    ) -> Result<(Workspace, bool), StoreError> {
        let conn = lock(&self.writer);
        let json = draft.map(serde_json::to_string).transpose()?;
        let changed = conn.execute(
            "UPDATE workspaces SET comment_draft = ?1 WHERE id = ?2 AND comment_draft IS NOT ?1",
            params![json, id.as_str()],
        )? > 0;
        Ok((workspace_on(&conn, id)?, changed))
    }

    /// Lets go of a file a client added, as [`let_go`] says. Says whether it did, so the caller lets go of its bytes
    /// too.
    pub fn let_go_of_attachment(&self, session_id: &SessionId, id: &AttachmentId) -> Result<bool, StoreError> {
        let_go(&lock(&self.writer), session_id, id)
    }
}

/// For a session being closed, inside the change that closes it: lets go of each file its draft named, as [`let_go`]
/// says, once the draft has gone with its composer, and of any decision being drafted on its request. Returns the files
/// let go of.
pub(in super::super) fn forget_drafts(conn: &Connection, id: &SessionId) -> Result<Vec<AttachmentId>, StoreError> {
    let mut named = conn.prepare(
        "SELECT json_extract(named.value, '$.id') FROM sessions, json_each(sessions.draft, '$.files') AS named
             WHERE sessions.id = ?",
    )?;
    let files: Vec<AttachmentId> = named
        .query_map([id.as_str()], OneValue::read::<String>)?
        .map(|file| file.map(Into::into))
        .collect::<Result<_, _>>()?;
    conn.execute("UPDATE sessions SET draft = NULL, decision_draft = NULL WHERE id = ?", [id.as_str()])?;
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
