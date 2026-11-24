use super::super::accounts::name_account;
use super::super::drafts::forget_drafts;
use super::super::made::{Made, inserted, made_by};
use super::super::{
    ByColumn, Columns, OneValue, Store, StoreError, StoredSession, columns, current_rev, json_at, lock, maybe_text_at,
    required_json_at, rev_of, seq_column, seq_of, text_at, text_of,
};
use protocol::capability::approvals::ApprovalMode;
use protocol::capability::settings::Settings;
use protocol::model::{
    Account, AccountId, AccountSource, AgentKind, AttachmentId, ClientRequestId, Draft, Interaction, ModelName,
    QueuedPrompt, Rev, RunId, Session, SessionId, SessionStatus, Title, Wait, WorkspaceId,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};

/// What moving a session to another account changed besides it, as [`Store::move_session`] says.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Moved {
    /// The record of the account it names now, when naming it brought when the account was last used up to the
    /// session's last send, which is a change to the account's record to announce.
    pub(crate) account: Option<Account>,
    /// Whether what the cache holds of the conversation is to be forgotten, as [`forgets_cache`] says.
    pub(crate) forgot_cache: bool,
}

columns! {
    /// What a session's row holds, as it's written, then what a read adds: the waits it has open, as JSON, which are in
    /// the waits table, since a session's waits outlive each other, and its revision, which the waits' triggers move
    /// too.
    pub(in super::super) struct SessionColumns {
        id, workspace_id, agent, model, mode, status, last_seq, created_at_ms, updated_at_ms, agent_session_id,
        agent_opened, title, closed_at_ms, reply_seq, read_seq, background_tasks, title_source, queued, queue_held,
        turn_started_at_ms, effort, account_id, user_active_at_ms, draft, decision_draft, untaken, open_approvals,
        reply_outcome, agent_capabilities, account_source, placed_automatically, needs_an_account, conversation,
    } then {
        waits = "(SELECT json_group_array(json_object('id', id, 'session_id', session_id, 'workspace_id', workspace_id,
            'number', number, 'call', call, 'until', until, 'started_at_ms', started_at_ms, 'watching', json(watch)))
            FROM (SELECT * FROM waits WHERE waits.session_id = sessions.id AND ended_at_ms IS NULL
                ORDER BY started_at_ms, id))",
        rev,
    }
    written as SessionValues with { client_request_id }
}

/// A draft for its column: NULL when nothing is typed.
pub(in super::super) fn draft_column(draft: &Draft) -> serde_json::Result<Option<String>> {
    (!draft.is_empty()).then(|| serde_json::to_string(draft)).transpose()
}

/// The session in a row of [`SessionColumns`].
pub(in super::super) fn session_from_row(row: &Row) -> rusqlite::Result<StoredSession> {
    let c = SessionColumns::at(0);
    // Written by the query, from the waits table, so one it can't read is a query that doesn't match `Wait`.
    let waits: Vec<Wait> = required_json_at(row, c.waits)?;
    Ok(StoredSession {
        session: Session {
            id: row.at::<String>(c.id)?.into(),
            workspace_id: row.at::<String>(c.workspace_id)?.into(),
            agent: text_at::<AgentKind>(row, c.agent)?,
            model: text_at(row, c.model)?,
            mode: required_json_at::<ApprovalMode>(row, c.mode)?,
            status: text_at(row, c.status)?,
            last_seq: seq_of(row, c.last_seq)?,
            created_at_ms: row.at(c.created_at_ms)?,
            updated_at_ms: row.at(c.updated_at_ms)?,
            title: row.at::<Option<String>>(c.title)?.map(Title::stored),
            closed_at_ms: row.at(c.closed_at_ms)?,
            reply_seq: seq_of(row, c.reply_seq)?,
            // An outcome this build doesn't know is `Unknown`, as any is.
            reply_outcome: maybe_text_at(row, c.reply_outcome)?,
            read_seq: seq_of(row, c.read_seq)?,
            // Written only by this hub, as JSON. Anything it can't read is nothing it knows to be running.
            background_tasks: json_at(row, c.background_tasks)?.unwrap_or_default(),
            title_source: text_at(row, c.title_source)?,
            queued: required_json_at::<Vec<QueuedPrompt>>(row, c.queued)?,
            queue_held: row.at(c.queue_held)?,
            placed_automatically: row.at(c.placed_automatically)?,
            needs_an_account: row.at(c.needs_an_account)?,
            // The device that has the conversation, written only by this hub, as JSON, while the mode is on. One it
            // can't read is as good as the mode being off.
            interaction: json_at(row, c.conversation)?
                .map_or(Interaction::Typing, |device| Interaction::Conversation { device }),
            turn_started_at_ms: row.at(c.turn_started_at_ms)?,
            effort: text_at(row, c.effort)?,
            account: row.at::<Option<String>>(c.account_id)?.map(AccountId::from),
            account_source: maybe_text_at::<AccountSource>(row, c.account_source)?,
            user_active_at_ms: row.at(c.user_active_at_ms)?,
            // Written only by this hub, as JSON. One it can't read is as good as nothing typed.
            draft: json_at(row, c.draft)?.unwrap_or_default(),
            // The same.
            decision_draft: json_at(row, c.decision_draft)?,
            // Written only by this hub, as JSON. One it can't read says nothing, which leaves a client to look in the
            // log, as one from an older hub does.
            untaken: json_at(row, c.untaken)?,
            open_approvals: json_at(row, c.open_approvals)?,
            agent_opened: Some(row.at(c.agent_opened)?),
            agent_capabilities: json_at(row, c.agent_capabilities)?,
            wait: waits.first().cloned(),
            waits: Some(waits),
            rev: rev_of(row, c.rev)?,
        },
        agent_session_id: row.at::<Option<String>>(c.agent_session_id)?.map(Into::into),
        agent_opened: row.at(c.agent_opened)?,
    })
}

impl Store {
    /// Returns the session as stored, with its revision. Making it is when the person last did something in its
    /// workspace, which moves in the same commit.
    pub fn insert_session(
        &self,
        stored: &StoredSession,
        client_request_id: Option<&ClientRequestId>,
    ) -> Result<Made<StoredSession>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        if let Some(made) = made_by(&tx, client_request_id)? {
            return Ok(Made::Earlier(made));
        }
        let session = &stored.session;
        let values = SessionValues {
            id: &session.id.as_str(),
            workspace_id: &session.workspace_id.as_str(),
            agent: &text_of(&session.agent),
            model: &session.model.as_str(),
            mode: &serde_json::to_string(&session.mode)?,
            status: &text_of(&session.status),
            last_seq: &seq_column(session.last_seq),
            created_at_ms: &session.created_at_ms,
            updated_at_ms: &session.updated_at_ms,
            agent_session_id: &stored.agent_session_id.as_ref().map(protocol::model::AgentSessionId::as_str),
            agent_opened: &stored.agent_opened,
            title: &session.title.as_deref(),
            closed_at_ms: &session.closed_at_ms,
            reply_seq: &seq_column(session.reply_seq),
            read_seq: &seq_column(session.read_seq),
            background_tasks: &serde_json::to_string(&session.background_tasks)?,
            title_source: &text_of(&session.title_source),
            queued: &serde_json::to_string(&session.queued)?,
            queue_held: &session.queue_held,
            turn_started_at_ms: &session.turn_started_at_ms,
            effort: &text_of(&session.effort),
            account_id: &session.account.as_ref().map(AccountId::as_str),
            user_active_at_ms: &session.user_active_at_ms,
            draft: &draft_column(&session.draft)?,
            decision_draft: &session.decision_draft.as_ref().map(serde_json::to_string).transpose()?,
            untaken: &serde_json::to_string(session.untaken.as_deref().unwrap_or_default())?,
            open_approvals: &serde_json::to_string(session.open_approvals.as_deref().unwrap_or_default())?,
            reply_outcome: &session.reply_outcome.as_ref().map(text_of),
            agent_capabilities: &session.agent_capabilities.as_ref().map(serde_json::to_string).transpose()?,
            account_source: &session.account_source.as_ref().map(text_of),
            placed_automatically: &session.placed_automatically,
            needs_an_account: &session.needs_an_account,
            conversation: &session.interaction.device().map(serde_json::to_string).transpose()?,
            client_request_id: &client_request_id.map(ClientRequestId::as_str),
        };
        tx.execute(&format!("INSERT INTO sessions {}", SessionValues::INSERT), values.params())?;
        // Making a chat is doing something in its workspace.
        tx.execute(
            "UPDATE workspaces SET user_active_at_ms = ? WHERE id = ?",
            params![session.created_at_ms, session.workspace_id.as_str()],
        )?;
        let made = inserted(&tx, session.id.as_str())?;
        tx.commit()?;
        Ok(made)
    }

    pub fn get_session(&self, id: &SessionId) -> Result<StoredSession, StoreError> {
        lock(&self.reader)
            .query_row(
                &format!("SELECT {} FROM sessions WHERE id = ?", SessionColumns::LIST),
                [id.as_str()],
                session_from_row,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "session", id: id.to_string() })
    }

    /// The run whose worker the session last recorded something from that the worker numbered, and its number.
    pub fn recorded_from_worker(&self, id: &SessionId) -> Result<Option<(RunId, u64)>, StoreError> {
        columns! {
            struct Recorded { recorded_run, recorded_through }
        }
        let c = Recorded::at(0);
        let found: Option<(Option<i64>, Option<i64>)> = lock(&self.reader)
            .query_row(&format!("SELECT {} FROM sessions WHERE id = ?", Recorded::LIST), [id.as_str()], |row| {
                Ok((row.at(c.recorded_run)?, row.at(c.recorded_through)?))
            })
            .optional()?;
        Ok(match found {
            Some((Some(run), Some(through))) => Some((RunId::new(run.cast_unsigned()), through.cast_unsigned())),
            _ => None,
        })
    }

    /// The workspace's sessions, as [`Session::oldest_first`] orders them.
    pub fn list_sessions(&self, workspace_id: &WorkspaceId) -> Result<Vec<Session>, StoreError> {
        let conn = lock(&self.reader);
        let mut statement = conn.prepare_cached(&format!(
            "SELECT {} FROM sessions WHERE workspace_id = ? ORDER BY created_at_ms, id",
            SessionColumns::LIST
        ))?;
        let rows = statement.query_map([workspace_id.as_str()], session_from_row)?;
        let sessions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(sessions.into_iter().map(|s| s.session).collect())
    }

    /// Keeps the model, mode and effort `change` sets, as [`set_settings`] says, where no event says it changed.
    pub fn set_session_settings(&self, id: &SessionId, change: &Settings) -> Result<(), StoreError> {
        set_settings(&lock(&self.writer), id, change)
    }

    /// Moves the session to `account`: puts it on the hub's login for it, which its record names, since no agent under
    /// another is left and its next launch runs under it, so the limits shown, and what the person sends from here, are
    /// that account's, and says whether another account's cache is to be forgotten, as [`forgets_cache`] says. One transaction, so
    /// a hub that stops partway leaves the session as it was.
    pub(crate) fn move_session(&self, id: &SessionId, account: &AccountId) -> Result<Moved, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        // Only where it changes, since the revision moves on whatever sets it, even to what it was.
        tx.execute(
            "UPDATE sessions SET account_source = 'hub' WHERE id = ? AND account_source IS NOT 'hub'",
            [id.as_str()],
        )?;
        // The person chose it, so Automatic no longer did, and it's on an account.
        tx.execute(
            "UPDATE sessions SET placed_automatically = 0, needs_an_account = 0
                 WHERE id = ? AND (placed_automatically OR needs_an_account)",
            [id.as_str()],
        )?;
        let named = name_account(&tx, id, Some(account))?;
        let forgot_cache = forgets_cache(named.before.as_ref(), account);
        tx.commit()?;
        Ok(Moved { account: named.account, forgot_cache })
    }

    /// Closes the session, or opens it again with `None`. Its draft goes as it closes, with its composer, and so do any
    /// answer being typed to its question, and each file the draft named that nothing else will send, which this
    /// returns, so the caller lets go of their bytes too. One transaction, so a draft kept meanwhile either comes first
    /// and goes too, or finds the session closed.
    pub fn set_session_closed(
        &self,
        id: &SessionId,
        closed_at_ms: Option<i64>,
    ) -> Result<Vec<AttachmentId>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute("UPDATE sessions SET closed_at_ms = ? WHERE id = ?", params![closed_at_ms, id.as_str()])?;
        let let_go_of = if closed_at_ms.is_some() { forget_drafts(&tx, id)? } else { Vec::new() };
        tx.commit()?;
        Ok(let_go_of)
    }

    /// Deletes a session nothing has been sent in, with nothing in its log and nothing queued, along with what it
    /// knows of any files added to it, which no prompt carried. Returns it as it was, and the revision deleting it
    /// made, or `None`, leaving it be, once anything has been sent. One transaction, so a prompt being recorded either
    /// comes first and keeps the session, or finds it gone.
    pub fn delete_unused_session(&self, id: &SessionId) -> Result<Option<(Session, Rev)>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(stored) = tx
            .query_row(
                &format!("SELECT {} FROM sessions WHERE id = ?", SessionColumns::LIST),
                [id.as_str()],
                session_from_row,
            )
            .optional()?
        else {
            return Err(StoreError::NotFound { kind: "session", id: id.to_string() });
        };
        let logged: bool =
            tx.query_row("SELECT EXISTS (SELECT 1 FROM events WHERE session_id = ?)", [id.as_str()], OneValue::read)?;
        if logged || !stored.session.queued.is_empty() {
            return Ok(None);
        }
        tx.execute("DELETE FROM attachments WHERE session_id = ?", [id.as_str()])?;
        tx.execute("DELETE FROM sessions WHERE id = ?", [id.as_str()])?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok(Some((stored.session, rev)))
    }

    /// Sessions whose agent was at work when the hub last stopped, in a turn or in the background.
    pub fn active_session_ids(&self) -> Result<Vec<SessionId>, StoreError> {
        let conn = lock(&self.reader);
        columns! {
            struct Active { id, status }
        }
        let c = Active::at(0);
        let mut statement = conn.prepare_cached(&format!("SELECT {} FROM sessions", Active::LIST))?;
        let rows = statement.query_map([], |row| {
            Ok((SessionId::from(row.at::<String>(c.id)?), text_at::<SessionStatus>(row, c.status)?))
        })?;
        let mut active = Vec::new();
        for row in rows {
            let (id, status) = row?;
            if status.is_active() {
                active.push(id);
            }
        }
        Ok(active)
    }
}

/// Whether what the API's cache holds of `session`'s conversation, which goes on under `account` from here, where its
/// record named `before`, is to be forgotten. Another account's cache may not hold it, as one in another organization
/// doesn't, so after one, or after a record that named none, which can't say whose the cache was, nothing is cached,
/// and the first request under `account` isn't taken for one that found the cache expired. The hub keeps what the
/// cache holds, as an observation, so it forgets it, and the record doesn't change.
pub(in super::super) fn forgets_cache(before: Option<&AccountId>, account: &AccountId) -> bool {
    before != Some(account)
}

/// Keeps the model, mode and effort `change` sets, in one statement. One that's `None` is left as it is, and a chat
/// keeps no other.
pub(in super::super) fn set_settings(conn: &Connection, id: &SessionId, change: &Settings) -> Result<(), StoreError> {
    let Settings { model, mode, effort, .. } = change;
    conn.execute(
        "UPDATE sessions
             SET model = COALESCE(?, model), mode = COALESCE(?, mode), effort = COALESCE(?, effort)
             WHERE id = ?",
        params![
            model.as_ref().map(ModelName::as_str),
            mode.as_ref().map(serde_json::to_string).transpose()?,
            effort.as_ref().map(text_of),
            id.as_str()
        ],
    )?;
    Ok(())
}
