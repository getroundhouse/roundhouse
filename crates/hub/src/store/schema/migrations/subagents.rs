//! The migrations that record subagents, kept apart since they're long. Each has its place in `MIGRATIONS`.

/// The subagent each event is from, as the call that started it, or NULL for the agent's own, and its place among
/// the events from the same one, from 1. Only Claude had recorded events. Its subagents' messages and results name
/// the call in their bodies, but a subagent's other `user` lines only in the line itself, which is read only where
/// it's JSON. Rebuilt rather than altered, so `raw` stays last.
pub(super) const EVENTS_BY_SUBAGENT: &str = r"
    CREATE TABLE events_with_subagent (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        seq INTEGER NOT NULL,
        ts_ms INTEGER NOT NULL,
        kind TEXT NOT NULL,
        client_msg_id TEXT,
        run INTEGER,
        subagent TEXT,
        place INTEGER NOT NULL,
        body TEXT NOT NULL,
        raw TEXT,
        PRIMARY KEY (session_id, seq)
    );
    INSERT INTO events_with_subagent
        SELECT session_id, seq, ts_ms, kind, client_msg_id, run, subagent,
            row_number() OVER (PARTITION BY session_id, subagent ORDER BY seq), body, raw
        FROM (SELECT *, CASE
            WHEN kind IN ('assistant', 'agent_user') THEN body ->> '$.parent_tool_use_id'
            -- Nested, since SQLite doesn't say in which order it reads what an AND joins, and a line that isn't JSON
            -- would fail the migration.
            WHEN kind = 'agent_other' AND json_valid(raw) THEN CASE
                WHEN raw ->> '$.type' = 'user' AND json_type(raw, '$.parent_tool_use_id') = 'text'
                    THEN raw ->> '$.parent_tool_use_id'
            END
        END AS subagent FROM events);
    DROP TABLE events;
    ALTER TABLE events_with_subagent RENAME TO events;
    CREATE UNIQUE INDEX events_client_msg ON events (session_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
    CREATE INDEX events_by_kind ON events (session_id, kind);
    CREATE INDEX events_by_subagent ON events (session_id, subagent, place);
";
