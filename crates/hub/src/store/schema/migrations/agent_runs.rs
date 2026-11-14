//! Conversation identity and claims come from the run's initialization, committed with that event.

pub(super) const AGENT_RUNS: &str = r"
    ALTER TABLE sessions ADD COLUMN chosen_conversation TEXT;
    UPDATE sessions SET chosen_conversation = agent_session_id;
    ALTER TABLE sessions DROP COLUMN agent_session_id;
    ALTER TABLE sessions RENAME COLUMN chosen_conversation TO agent_session_id;
    ALTER TABLE sessions ADD COLUMN agent_capabilities TEXT;
    CREATE TRIGGER sessions_rev_on_agent_capabilities
        AFTER UPDATE OF agent_capabilities ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
