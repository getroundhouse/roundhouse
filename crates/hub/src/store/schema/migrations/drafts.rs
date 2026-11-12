//! The migrations that keep what's typed and not sent, in each session's composer and in answer to its question, and
//! on each workspace's diff, kept apart since they're long. Each has its place in `MIGRATIONS`.

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

/// What's chosen and typed in answer to the question each session's agent is waiting on, as JSON, which is NULL when
/// nothing is. A Session record shows it, so the trigger names it.
pub(super) const ANSWER_DRAFT: &str = r"
    ALTER TABLE sessions ADD COLUMN answer_draft TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms, prompt_cache, draft, answer_draft
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";

/// The comment being written on each workspace's diff, as JSON, which is NULL when none is. Any change to a workspace
/// moves its revision, so its trigger needs nothing more.
pub(super) const COMMENT_DRAFT: &str = r"
    ALTER TABLE workspaces ADD COLUMN comment_draft TEXT;
";

/// Which files a client added, rather than the agent, since only those can be in a draft, and so only those can a
/// client have the hub let go of. Every file from before was a prompt's or an agent's, so each stays.
pub(super) const ADDED_BY_CLIENT: &str = r"
    ALTER TABLE attachments ADD COLUMN by_client INTEGER NOT NULL DEFAULT 0;
";
