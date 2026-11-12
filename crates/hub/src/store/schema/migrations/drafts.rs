//! The migrations that keep what's in each session's composer, kept apart since they're long. Each has its place in
//! `MIGRATIONS`.

/// What's typed and attached in each session's composer and not sent, as JSON, which is NULL when nothing is. A
/// Session record shows it, so the trigger names it.
pub(super) const DRAFT: &str = r"
    ALTER TABLE sessions ADD COLUMN draft TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms, prompt_cache, draft
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";

/// Which files a client added, rather than the agent, since only those can be in a draft, and so only those can a
/// client have the hub let go of. Every file from before was a prompt's or an agent's, so each stays.
pub(super) const ADDED_BY_CLIENT: &str = r"
    ALTER TABLE attachments ADD COLUMN by_client INTEGER NOT NULL DEFAULT 0;
";
