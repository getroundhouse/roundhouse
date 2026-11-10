//! The database's migrations, and how many there were before some of them, for tests of what migrating keeps.

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
    // How full each session's agent's context is, as its newest report said, as JSON. A session from before this has
    // none until its agent next says. A Session record shows it, so the trigger names it.
    r"
    ALTER TABLE sessions ADD COLUMN context TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // What agents have heard of each account's limits, one row for each window of each account, with when it was
    // heard. It's not a record the feed announces: a client reads it when it shows it.
    r"
    CREATE TABLE account_limits (
        account_id TEXT NOT NULL,
        window TEXT NOT NULL,
        used_percent INTEGER NOT NULL,
        resets_at_ms INTEGER,
        heard_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, window)
    );
",
    // The hub's own settings, one row of them, which is a record like the rest. It keeps its machine awake unless
    // someone has chosen otherwise. The row goes in after its trigger, so it has a revision from the start.
    r"
    CREATE TABLE hub_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        keep_awake INTEGER NOT NULL DEFAULT 1,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER hub_settings_rev_on_insert AFTER INSERT ON hub_settings BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE hub_settings SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
    CREATE TRIGGER hub_settings_rev_on_update AFTER UPDATE ON hub_settings WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE hub_settings SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
    INSERT INTO hub_settings (id) VALUES (1);
",
];

/// How many migrations there were before events recorded their run, so a test can make an older database.
#[cfg(test)]
pub(crate) const BEFORE_RUNS: usize = 4;

/// How many migrations there were before workspaces had titles.
#[cfg(test)]
pub(crate) const BEFORE_TITLES: usize = 9;

/// How many migrations there were before sessions had an effort.
#[cfg(test)]
pub(crate) const BEFORE_EFFORT: usize = 13;

/// How many migrations there were before workspaces kept their pull request.
#[cfg(test)]
pub(crate) const BEFORE_PULL_REQUESTS: usize = 14;

/// How many migrations there were before sessions kept how full their agent's context is.
#[cfg(test)]
pub(crate) const BEFORE_CONTEXT: usize = 15;

/// How many migrations there were before the hub kept settings of its own.
#[cfg(test)]
pub(crate) const BEFORE_HUB_SETTINGS: usize = 17;
