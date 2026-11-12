//! The migrations that record when the person last did something, kept apart since they're long. Each has its place in
//! `MIGRATIONS`.

/// When the person last sent each session's agent something. Filled in from the log for what came before, which can't
/// tell a prompt the hub sent from the queue from one sent at once, so it counts each when it was sent. After the
/// trigger, so each session it fills in gets a newer revision and a client's next list takes it.
pub(super) const BY_SESSION: &str = r"
    ALTER TABLE sessions ADD COLUMN user_active_at_ms INTEGER;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    UPDATE sessions SET user_active_at_ms = sent.at
        FROM (
            SELECT session_id, max(ts_ms) AS at FROM events
                WHERE run IS NULL AND (kind = 'user_prompt'
                    OR (kind = 'approval_resolved' AND body ->> '$.outcome' IN ('allowed', 'denied')))
                GROUP BY session_id
        ) AS sent
        WHERE sent.session_id = sessions.id;
";

/// When the person last did something in each workspace: made it, made a chat in it, or sent one of its agents
/// something. Filled in for what came before from the sessions, which the migration before filled in from the log.
/// Every update moves a workspace's revision, so a client's next list takes each one filled in.
pub(super) const BY_WORKSPACE: &str = r"
    ALTER TABLE workspaces ADD COLUMN user_active_at_ms INTEGER;
    UPDATE workspaces SET user_active_at_ms = max(created_at_ms, coalesce((
        SELECT max(max(created_at_ms, coalesce(user_active_at_ms, 0))) FROM sessions
            WHERE sessions.workspace_id = workspaces.id
    ), 0));
";
