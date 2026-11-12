//! The migration that keeps each agent's waits on its workspace's pull request, kept apart since it's long. It has its
//! place in `MIGRATIONS`.

/// The waits, which last across the hub's restarts. One that hasn't ended has no `ended_at_ms`, and a chat has one open
/// at a time. The `seen_` columns are what GitHub had recorded when it started, by GitHub's clock. Closing a chat or
/// archiving a workspace ends its waits as cancelled, in the same statement, so none is left open in either, whichever
/// way it's closed.
pub(super) const WAITS: &str = r"
    CREATE TABLE waits (
        id TEXT PRIMARY KEY,
        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        call TEXT,
        until TEXT NOT NULL,
        started_at_ms INTEGER NOT NULL,
        seen_left_queue_at_ms INTEGER,
        seen_changes_requested_at_ms INTEGER,
        ended_at_ms INTEGER,
        ended TEXT,
        CHECK ((ended_at_ms IS NULL) = (ended IS NULL))
    );
    CREATE INDEX waits_workspace ON waits (workspace_id, ended_at_ms);
    CREATE INDEX waits_open ON waits (workspace_id) WHERE ended_at_ms IS NULL;
    CREATE INDEX waits_session ON waits (session_id, call);
    CREATE UNIQUE INDEX waits_open_in_session ON waits (session_id) WHERE ended_at_ms IS NULL;
    CREATE TRIGGER waits_end_with_chat AFTER UPDATE OF closed_at_ms ON sessions
        WHEN NEW.closed_at_ms IS NOT NULL
    BEGIN
        UPDATE waits SET ended_at_ms = NEW.closed_at_ms, ended = 'cancelled'
            WHERE session_id = NEW.id AND ended_at_ms IS NULL;
    END;
    CREATE TRIGGER waits_end_with_workspace AFTER UPDATE OF status ON workspaces
        WHEN NEW.status = 'archived'
    BEGIN
        UPDATE waits
            SET ended_at_ms = coalesce(NEW.archived_at_ms, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)),
                ended = 'cancelled'
            WHERE workspace_id = NEW.id AND ended_at_ms IS NULL;
    END;
";

/// A session's record shows the wait it has open, which is in this table rather than the session's row, so starting or
/// ending one moves the session's revision, and the feed announces it. One that ended as it started was never open, so
/// it moves nothing. Ending one as its chat closes moves it again, which costs nothing.
pub(super) const SESSIONS_SHOW_WAITS: &str = r"
    CREATE TRIGGER waits_move_session_on_start AFTER INSERT ON waits WHEN NEW.ended_at_ms IS NULL BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.session_id;
    END;
    CREATE TRIGGER waits_move_session_on_end AFTER UPDATE OF ended_at_ms ON waits
        WHEN OLD.ended_at_ms IS NULL AND NEW.ended_at_ms IS NOT NULL
    BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.session_id;
    END;
";
