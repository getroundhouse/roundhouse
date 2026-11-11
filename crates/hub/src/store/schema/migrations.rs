//! The database's migrations. `before.rs` has how many there were before some of them, for tests.

mod first;
mod subagents;

/// Applied in order. `PRAGMA user_version` records how many have run.
pub(super) const MIGRATIONS: &[&str] = &[
    // What the hub began with.
    first::FIRST,
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
    // heard. It's not a record the feed announces: a client reads it when it shows it. Accounts became records later.
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
    // Every session names its model and effort from here on, so the agent's CLI never picks either for itself, and a
    // session that doesn't won't read. One that left either to the CLI gets what the hub gives a new session that names
    // neither. A model names none as `ModelName` says: empty, `default`, which was the picker's name for the CLI's own
    // choice, or with anything but printable ASCII, which the GLOB's range matches.
    r"
    UPDATE sessions SET model = 'opus'
        WHERE model IS NULL OR model = '' OR model = 'default' OR model GLOB '*[^!-~]*';
    UPDATE sessions SET effort = 'xhigh' WHERE effort IS NULL;
",
    // Each account agents run under is a record the feed announces, so the hub keeps who it is beside what its agents
    // have heard of its limits. `heard_at_ms` is when an agent under it last heard of any, and changes whenever what's
    // kept of them does, which is what gives the record a newer revision. Every account whose limits were kept gets
    // one, without who it is until an agent under it next launches. Each session names the account its agent's newest
    // launch runs under, which a Session record shows, so the trigger names it.
    r"
    CREATE TABLE accounts (
        id TEXT PRIMARY KEY,
        model_provider TEXT,
        plan TEXT,
        login TEXT,
        email TEXT,
        rev INTEGER NOT NULL DEFAULT 0
    );
    CREATE TRIGGER accounts_rev_on_insert AFTER INSERT ON accounts BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER accounts_rev_on_update AFTER UPDATE ON accounts WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    -- What's left of its limits is part of an account's record, so a change to it is a change to the record. Hearing
    -- the same again only says when it was heard, which is kept so something heard earlier never replaces it.
    CREATE TRIGGER account_limits_rev_on_insert AFTER INSERT ON account_limits BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.account_id;
    END;
    CREATE TRIGGER account_limits_rev_on_update AFTER UPDATE OF used_percent, resets_at_ms ON account_limits
        WHEN NEW.used_percent IS NOT OLD.used_percent OR NEW.resets_at_ms IS NOT OLD.resets_at_ms BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.account_id;
    END;
    INSERT INTO accounts (id) SELECT DISTINCT account_id FROM account_limits;
    ALTER TABLE sessions ADD COLUMN account_id TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
",
    // A chat closed before anything was sent in it is deleted from here on, since there's nothing to reopen, so the
    // ones closed that way before go too. One with a file added is left: its bytes are outside the database, where
    // only the hub can let them go. Deleting moves the revision, so a client's next list drops them.
    r"
    DELETE FROM sessions
        WHERE closed_at_ms IS NOT NULL AND queued = '[]'
            AND NOT EXISTS (SELECT 1 FROM events WHERE events.session_id = sessions.id)
            AND NOT EXISTS (SELECT 1 FROM attachments WHERE attachments.session_id = sessions.id);
",
    // The subagent each event is from, and its place among those from the same one. In `subagents`, as are the other
    // migrations that record subagents.
    subagents::EVENTS_BY_SUBAGENT,
    // Each subagent's record, filled in for those from before it from what the log kept.
    subagents::RECORDS,
    // Every session names its approval mode from here on, and every agent has full access unless it's planning. One
    // that left it to the user's own Claude settings, or chose a mode only the old picker offered, such as asking first,
    // gets full access.
    r"
    UPDATE sessions SET permission_mode = 'bypassPermissions'
        WHERE permission_mode IS NULL OR permission_mode NOT IN ('plan', 'bypassPermissions');
",
    // What each subagent has done lately, which its record shows while it works.
    subagents::RECENT_WORK,
];
