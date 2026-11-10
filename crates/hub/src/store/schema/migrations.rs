//! Every change to the database's schema since the first, in the order they run.

/// Applied in order. `PRAGMA user_version` records how many have run.
pub(super) const MIGRATIONS: &[&str] = &[
    r"
    CREATE TABLE repos (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        created_at_ms INTEGER NOT NULL
    );
    CREATE TABLE workspaces (
        id TEXT PRIMARY KEY,
        repo_id TEXT NOT NULL REFERENCES repos(id),
        name TEXT NOT NULL,
        branch TEXT NOT NULL,
        base_ref TEXT,
        path TEXT,
        status TEXT NOT NULL,
        provider TEXT NOT NULL,
        instance_id TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        archived_at_ms INTEGER,
        UNIQUE (repo_id, name)
    );
    CREATE TABLE sessions (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        agent TEXT NOT NULL,
        model TEXT,
        permission_mode TEXT,
        agent_session_id TEXT NOT NULL,
        agent_opened INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL,
        last_seq INTEGER NOT NULL DEFAULT 0,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    );
    CREATE INDEX sessions_by_workspace ON sessions (workspace_id, created_at_ms);
    -- raw is last, and SQLite reads columns lazily, so range reads don't touch it.
    CREATE TABLE events (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        seq INTEGER NOT NULL,
        ts_ms INTEGER NOT NULL,
        kind TEXT NOT NULL,
        client_msg_id TEXT,
        body TEXT NOT NULL,
        raw TEXT,
        PRIMARY KEY (session_id, seq)
    );
    CREATE UNIQUE INDEX events_client_msg ON events (session_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
    CREATE INDEX events_by_kind ON events (session_id, kind);
",
    r"
    CREATE TABLE diff_comments (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        path TEXT NOT NULL,
        side TEXT NOT NULL,
        line INTEGER NOT NULL,
        text TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
    CREATE INDEX diff_comments_by_workspace ON diff_comments (workspace_id, created_at_ms);
",
    r#"
    ALTER TABLE workspaces ADD COLUMN setup TEXT NOT NULL DEFAULT '{"state":"not_started"}';
    ALTER TABLE workspaces ADD COLUMN port_base INTEGER NOT NULL DEFAULT 0;
"#,
    r"
    ALTER TABLE sessions ADD COLUMN title TEXT;
    ALTER TABLE sessions ADD COLUMN closed_at_ms INTEGER;
",
    // Rebuilt rather than altered, so `run` comes before `raw` and `raw` stays last.
    r"
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
",
    // Each record's revision is set by triggers, so no write can forget it. A delete moves the counter too,
    // so a client can tell a removal from what came before it. Revisions start at 1, because 0 means a hub
    // that doesn't keep them.
    r"
    INSERT INTO counters (name, value) VALUES ('rev', 1);
    ALTER TABLE repos ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
    UPDATE repos SET rev = 1;
    CREATE TRIGGER repos_rev_on_insert AFTER INSERT ON repos BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE repos SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER repos_rev_on_update AFTER UPDATE ON repos WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE repos SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER repos_rev_on_delete AFTER DELETE ON repos BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
    ALTER TABLE workspaces ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
    UPDATE workspaces SET rev = 1;
    CREATE TRIGGER workspaces_rev_on_insert AFTER INSERT ON workspaces BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE workspaces SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER workspaces_rev_on_update AFTER UPDATE ON workspaces WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE workspaces SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER workspaces_rev_on_delete AFTER DELETE ON workspaces BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
    ALTER TABLE sessions ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
    UPDATE sessions SET rev = 1;
    CREATE TRIGGER sessions_rev_on_insert AFTER INSERT ON sessions BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    -- Not on every event, which moves last_seq and updated_at_ms but isn't announced on the feed. A column
    -- that a Session record shows, and the feed announces, has to be named here.
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER sessions_rev_on_delete AFTER DELETE ON sessions BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
    ALTER TABLE diff_comments ADD COLUMN rev INTEGER NOT NULL DEFAULT 0;
    UPDATE diff_comments SET rev = 1;
    CREATE TRIGGER diff_comments_rev_on_insert AFTER INSERT ON diff_comments BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE diff_comments SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER diff_comments_rev_on_update AFTER UPDATE ON diff_comments WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE diff_comments SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER diff_comments_rev_on_delete AFTER DELETE ON diff_comments BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
",
    // What a client chose for a request that created a record, so sending it again returns that record.
    r"
    ALTER TABLE workspaces ADD COLUMN client_request_id TEXT;
    ALTER TABLE sessions ADD COLUMN client_request_id TEXT;
    ALTER TABLE diff_comments ADD COLUMN client_request_id TEXT;
    CREATE UNIQUE INDEX workspaces_by_request ON workspaces (client_request_id) WHERE client_request_id IS NOT NULL;
    CREATE UNIQUE INDEX sessions_by_request ON sessions (client_request_id) WHERE client_request_id IS NOT NULL;
    CREATE UNIQUE INDEX diff_comments_by_request ON diff_comments (client_request_id)
        WHERE client_request_id IS NOT NULL;
",
    // Each session's newest reply, and how far it's been read. A session from before this has no reply, so nothing
    // turns unread by updating. Both are columns a Session record shows, so the trigger names them.
    r"
    ALTER TABLE sessions ADD COLUMN reply_seq INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN read_seq INTEGER NOT NULL DEFAULT 0;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // What each session's agent is running in the background, as JSON. A session from before this had nothing
    // running that's still going, since a new hub's agents are all new. The Session record shows it, so the trigger
    // names it.
    r"
    ALTER TABLE sessions ADD COLUMN background_tasks TEXT NOT NULL DEFAULT '[]';
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // Titles, and where each came from. A workspace from before takes its oldest chat's title, which is that chat's
    // first prompt's first line, so whatever is said in it next doesn't become its title. Where a session's title
    // came from is a column its record shows, so the trigger names it.
    r"
    ALTER TABLE workspaces ADD COLUMN title TEXT;
    ALTER TABLE workspaces ADD COLUMN title_source TEXT NOT NULL DEFAULT 'first_line';
    ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT 'first_line';
    UPDATE workspaces SET title = (
        SELECT title FROM sessions
        WHERE sessions.workspace_id = workspaces.id AND title IS NOT NULL
        ORDER BY created_at_ms, rowid LIMIT 1
    )
    WHERE EXISTS (SELECT 1 FROM sessions WHERE sessions.workspace_id = workspaces.id AND title IS NOT NULL);
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // The files a session's prompts carry. What each is never changes once added, and where it was put is filled in
    // once. Each prompt's event says which it carries.
    // Their bytes are files in the attachments directory, so copying the database before a migration stays quick.
    r"
    CREATE TABLE attachments (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        id TEXT NOT NULL,
        name TEXT NOT NULL,
        media_type TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        created_at_ms INTEGER NOT NULL,
        -- Where the worker put it in the worktree, once a prompt has carried it.
        placed_at TEXT,
        PRIMARY KEY (session_id, id)
    );
",
    // Each session's queue, as a JSON list, and whether it's held. A Session record shows both, so the trigger names
    // them too. Also the prompts its agent hasn't taken or dropped yet, as a JSON list of where they're recorded, so a
    // hub that starts again can drop those its last one's agents took with them. Only the hub reads that, so the
    // trigger leaves it out.
    r"
    ALTER TABLE sessions ADD COLUMN queued TEXT NOT NULL DEFAULT '[]';
    ALTER TABLE sessions ADD COLUMN queue_held INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE sessions ADD COLUMN untaken TEXT NOT NULL DEFAULT '[]';
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // When each session's newest turn started. A session from before this has none until its next turn. It's a column
    // a Session record shows, so the trigger names it.
    r"
    ALTER TABLE sessions ADD COLUMN turn_started_at_ms INTEGER;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // How hard each session's agent thinks, as the CLI names the level. A session from before this has none, which
    // leaves it to the user's own settings, as it always did. A Session record shows it, so the trigger names it.
    r"
    ALTER TABLE sessions ADD COLUMN effort TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // What the hub last found of each workspace's pull request, as JSON, which is `null` when it found none. NULL until
    // it has looked, as for every workspace from before this. The workspaces' trigger takes every column.
    r"
    ALTER TABLE workspaces ADD COLUMN pull_request TEXT;
",
    // The subagent each event is from, as the call that started it, or NULL for the agent's own, and its place among
    // the events from the same one, from 1. Only Claude had recorded events. Its subagents' messages and results name
    // the call in their bodies, but a subagent's other `user` lines only in the line itself, which is read only where
    // it's JSON. Rebuilt rather than altered, so `raw` stays last.
    r"
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
            WHEN kind = 'agent_other' AND json_valid(raw) AND raw ->> '$.type' = 'user'
                AND json_type(raw, '$.parent_tool_use_id') = 'text' THEN raw ->> '$.parent_tool_use_id'
        END AS subagent FROM events);
    DROP TABLE events;
    ALTER TABLE events_with_subagent RENAME TO events;
    CREATE UNIQUE INDEX events_client_msg ON events (session_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
    CREATE INDEX events_by_kind ON events (session_id, kind);
    CREATE INDEX events_by_subagent ON events (session_id, subagent, place);
",
    // Each subagent's record, with a revision like every record's. One from before this is filled in as the hub would
    // have kept it: from the CLI's word on the task it ran as, which the log kept whole, and from its own calls, which
    // count from its start. It ended when the CLI said so or its agent exited, whichever came first, or else it went
    // with its agent before this hub started. A field is read only when it's text, as the adapter reads it.
    r"
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
    CREATE TEMP TABLE started AS SELECT s.session_id, s.seq, s.ts_ms, s.call, s.raw,
            (SELECT min(x.seq) FROM exits x WHERE x.session_id = s.session_id AND x.seq > s.seq) AS exited,
            (SELECT min(n.seq) FROM tasks n WHERE n.session_id = s.session_id AND n.seq > s.seq
                AND n.said = 'task_notification' AND n.task = s.task) AS notified
        FROM tasks s WHERE s.said = 'task_started' AND s.task_type = 'local_agent' AND s.call IS NOT NULL;
    CREATE TEMP TABLE made AS SELECT e.session_id, e.subagent AS call, e.seq, b.key AS position,
            json_object('id', b.value ->> '$.id', 'name', b.value ->> '$.name', 'input', b.value -> '$.input') AS made
        FROM events e, json_each(e.body, '$.blocks') b
        WHERE e.kind = 'assistant' AND e.subagent IS NOT NULL AND b.value ->> '$.type' = 'tool_use';
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
            s.ts_ms, coalesce(n.ts_ms, x.ts_ms),
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
",
];
