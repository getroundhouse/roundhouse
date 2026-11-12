//! The migrations that keep what's typed and not sent, kept apart since they're long. Each has its place in
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

/// What's in the new-workspace dialog and not made yet, one row of it, as JSON, which is NULL until something's kept.
/// It's a record of its own, so it has a revision of its own, from the start as the hub's settings do. Its files wait
/// in a table of their own, since there's no session yet to add them to.
pub(super) const WORKSPACE_DRAFT: &str = r"
    CREATE TABLE workspace_draft (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        draft TEXT,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER workspace_draft_rev_on_insert AFTER INSERT ON workspace_draft BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE workspace_draft SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
    CREATE TRIGGER workspace_draft_rev_on_update AFTER UPDATE ON workspace_draft WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE workspace_draft SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
    INSERT INTO workspace_draft (id) VALUES (1);
    CREATE TABLE workspace_draft_files (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        media_type TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
";
