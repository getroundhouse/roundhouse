//! The to-do list each chat's agent keeps.

/// The newest to-do list each session's agent reported, as JSON, which its record shows, so it moves the revision. A
/// session from before has none, until its agent next says.
pub(super) const TODOS: &str = r"
    ALTER TABLE sessions ADD COLUMN todos TEXT;
    CREATE TRIGGER sessions_rev_on_todos
        AFTER UPDATE OF todos ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
