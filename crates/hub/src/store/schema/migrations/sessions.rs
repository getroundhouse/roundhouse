//! What each session's record no longer keeps, since the hub keeps it as an observation.

/// How full each session's agent's context is, and its to-do list, which the hub keeps as observations, in its cache
/// file, and no longer here. Nothing of either is carried there: each agent says them again as it next works, which is
/// its chat's next turn. The trigger that moves a session's revision on a change to its columns is made again without
/// `context`, since SQLite drops no column a trigger names.
pub(super) const CONTEXT_OBSERVED: &str = r"
    DROP TRIGGER sessions_rev_on_todos;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, account_id,
            user_active_at_ms, prompt_cache, draft, decision_draft, untaken, agent_opened, open_approvals, reply_outcome
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    ALTER TABLE sessions DROP COLUMN context;
    ALTER TABLE sessions DROP COLUMN todos;
";
