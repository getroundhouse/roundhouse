//! The migration that records the run each event was recorded in, which has its place in `MIGRATIONS`.

/// The run each event was recorded in, which `counters` numbers. `events` is rebuilt rather than
/// altered, so `run` comes before `raw` and `raw` stays last.
pub(super) const EVENTS_WITH_RUN: &str = r"
    CREATE TABLE events_with_run (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        seq INTEGER NOT NULL,
        ts_ms INTEGER NOT NULL,
        kind TEXT NOT NULL,
        client_msg_id TEXT,
        run INTEGER,
        body TEXT NOT NULL,
        raw TEXT,
        PRIMARY KEY (session_id, seq)
    );
    INSERT INTO events_with_run (session_id, seq, ts_ms, kind, client_msg_id, body, raw)
        SELECT session_id, seq, ts_ms, kind, client_msg_id, body, raw FROM events;
    DROP TABLE events;
    ALTER TABLE events_with_run RENAME TO events;
    CREATE UNIQUE INDEX events_client_msg ON events (session_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
    CREATE INDEX events_by_kind ON events (session_id, kind);
    CREATE TABLE counters (
        name TEXT PRIMARY KEY,
        value INTEGER NOT NULL
    );
    INSERT INTO counters (name, value) VALUES ('run', 0);
";
