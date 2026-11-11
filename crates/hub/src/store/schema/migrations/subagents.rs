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

/// Each subagent's record, with a revision like every record's. One from before this is filled in as the hub would
/// have kept it: from the CLI's word on the task it ran as, which the log kept whole, and from its own calls, which
/// count from its start. It ended when the CLI said so or its agent exited, whichever came first, or else it went
/// with its agent before this hub started, as of its session's last event. A field is read only when it's text, as
/// the adapter reads it.
pub(super) const RECORDS: &str = r"
    CREATE TABLE subagents (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        call TEXT NOT NULL,
        kind TEXT,
        description TEXT,
        prompt TEXT,
        in_background INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL,
        started_at_ms INTEGER NOT NULL,
        ended_at_ms INTEGER,
        calls INTEGER NOT NULL DEFAULT 0,
        latest_call TEXT,
        report TEXT,
        rev INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (session_id, call)
    );
    CREATE TRIGGER subagents_rev_on_insert AFTER INSERT ON subagents BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE subagents SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER subagents_rev_on_update AFTER UPDATE ON subagents WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE subagents SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TEMP TABLE tasks AS SELECT session_id, seq, ts_ms, raw ->> '$.subtype' AS said,
            CASE json_type(raw, '$.task_id') WHEN 'text' THEN raw ->> '$.task_id' END AS task,
            CASE json_type(raw, '$.tool_use_id') WHEN 'text' THEN raw ->> '$.tool_use_id' END AS call,
            raw ->> '$.task_type' AS task_type, raw
        FROM events
        WHERE kind IN ('agent_other', 'subagent_started', 'subagent_ended') AND CASE WHEN json_valid(raw)
            THEN raw ->> '$.type' = 'system' AND raw ->> '$.subtype' IN ('task_started', 'task_notification')
            ELSE 0 END;
    CREATE TEMP TABLE exits AS SELECT session_id, seq, ts_ms FROM events WHERE kind = 'agent_exited';
    -- Indexed, so finding each subagent's end and calls takes time with the log, not with subagents times calls.
    CREATE INDEX tasks_by_task ON tasks (session_id, task, seq);
    CREATE INDEX tasks_by_seq ON tasks (session_id, seq);
    CREATE INDEX exits_by_seq ON exits (session_id, seq);
    CREATE TEMP TABLE started AS SELECT s.session_id, s.seq, s.ts_ms, s.call, s.raw,
            (SELECT min(x.seq) FROM exits x WHERE x.session_id = s.session_id AND x.seq > s.seq) AS exited,
            (SELECT min(n.seq) FROM tasks n WHERE n.session_id = s.session_id AND n.seq > s.seq
                AND n.said = 'task_notification' AND n.task = s.task) AS notified
        FROM tasks s WHERE s.said = 'task_started' AND s.task_type = 'local_agent' AND s.call IS NOT NULL;
    CREATE TEMP TABLE made AS SELECT e.session_id, e.subagent AS call, e.seq, b.key AS position,
            json_object('id', b.value ->> '$.id', 'name', b.value ->> '$.name', 'input', b.value -> '$.input') AS made
        FROM events e, json_each(e.body, '$.blocks') b
        WHERE e.kind = 'assistant' AND e.subagent IS NOT NULL AND b.value ->> '$.type' = 'tool_use';
    CREATE INDEX made_by_call ON made (session_id, call, seq, position);
    INSERT OR IGNORE INTO subagents
        (session_id, call, kind, description, prompt, in_background, status, started_at_ms, ended_at_ms, calls,
            latest_call, report)
        SELECT s.session_id, s.call,
            CASE json_type(s.raw, '$.subagent_type') WHEN 'text' THEN s.raw ->> '$.subagent_type' END,
            CASE json_type(s.raw, '$.description') WHEN 'text' THEN s.raw ->> '$.description' END,
            CASE json_type(s.raw, '$.prompt') WHEN 'text' THEN s.raw ->> '$.prompt' END,
            json_type(s.raw, '$.is_backgrounded') IS 'true',
            CASE WHEN n.seq IS NULL THEN 'stopped' ELSE CASE n.raw ->> '$.status' WHEN 'completed' THEN 'finished'
                WHEN 'failed' THEN 'failed' WHEN 'stopped' THEN 'stopped' ELSE 'unknown' END END,
            s.ts_ms,
            coalesce(n.ts_ms, x.ts_ms,
                (SELECT e.ts_ms FROM events e WHERE e.session_id = s.session_id ORDER BY e.seq DESC LIMIT 1)),
            (SELECT count(*) FROM made m WHERE m.session_id = s.session_id AND m.call = s.call AND m.seq > s.seq),
            (SELECT m.made FROM made m WHERE m.session_id = s.session_id AND m.call = s.call AND m.seq > s.seq
                ORDER BY m.seq DESC, m.position DESC LIMIT 1),
            CASE WHEN n.raw ->> '$.status' = 'completed' AND json_type(n.raw, '$.summary') = 'text'
                THEN n.raw ->> '$.summary' END
        FROM started s
            LEFT JOIN tasks n ON n.session_id = s.session_id AND n.seq = s.notified
                AND (s.exited IS NULL OR s.notified < s.exited)
            LEFT JOIN exits x ON x.session_id = s.session_id AND x.seq = s.exited
        ORDER BY s.seq;
    DROP TABLE tasks;
    DROP TABLE exits;
    DROP TABLE started;
    DROP TABLE made;
";
