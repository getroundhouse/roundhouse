//! The migration that keeps what the API's cache holds of each session's conversation, kept apart since it's long. It
//! has its place in `MIGRATIONS`.

/// What the API's cache holds of each session's conversation, as its agent's newest request left it, which the record
/// shows, so it moves the revision.
pub(super) const PROMPT_CACHE: &str = r"
    ALTER TABLE sessions ADD COLUMN prompt_cache TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms, prompt_cache
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
