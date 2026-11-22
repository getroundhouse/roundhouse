-- A database as the first 71 of the hub's migrations left it, a migration short of release 0.1.870's, holding a
-- repository, as `sqlite3 .dump` wrote it. Made with those migrations, before they became one schema.
PRAGMA foreign_keys=OFF;
BEGIN TRANSACTION;
CREATE TABLE repos (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        path TEXT NOT NULL UNIQUE,
        created_at_ms INTEGER NOT NULL
    , rev INTEGER NOT NULL DEFAULT 0, url TEXT, url_to_read INTEGER NOT NULL DEFAULT 1, push_around INTEGER);
INSERT INTO repos VALUES('r','rails','/src/rails',1000,4,NULL,1,NULL);
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
        archived_at_ms INTEGER, setup TEXT NOT NULL DEFAULT '{"state":"not_started"}', port_base INTEGER NOT NULL DEFAULT 0, rev INTEGER NOT NULL DEFAULT 0, client_request_id TEXT, title TEXT, title_source TEXT NOT NULL DEFAULT 'first_line', pull_request TEXT, user_active_at_ms INTEGER, comment_draft TEXT, branches TEXT, pull_request_branch TEXT, worker_build TEXT,
        UNIQUE (repo_id, name)
    );
CREATE TABLE sessions (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        agent TEXT NOT NULL,
        model TEXT,
        permission_mode TEXT,
        agent_opened INTEGER NOT NULL DEFAULT 0,
        status TEXT NOT NULL,
        last_seq INTEGER NOT NULL DEFAULT 0,
        created_at_ms INTEGER NOT NULL,
        updated_at_ms INTEGER NOT NULL
    , title TEXT, closed_at_ms INTEGER, rev INTEGER NOT NULL DEFAULT 0, client_request_id TEXT, reply_seq INTEGER NOT NULL DEFAULT 0, read_seq INTEGER NOT NULL DEFAULT 0, background_tasks TEXT NOT NULL DEFAULT '[]', title_source TEXT NOT NULL DEFAULT 'first_line', queued TEXT NOT NULL DEFAULT '[]', queue_held INTEGER NOT NULL DEFAULT 0, untaken TEXT NOT NULL DEFAULT '[]', turn_started_at_ms INTEGER, effort TEXT, context TEXT, account_id TEXT, user_active_at_ms INTEGER, prompt_cache TEXT, draft TEXT, decision_draft TEXT, recorded_run INTEGER, recorded_through INTEGER, open_approvals TEXT NOT NULL DEFAULT '[]', reply_outcome TEXT, agent_session_id TEXT, agent_capabilities TEXT, account_source TEXT);
CREATE TABLE diff_comments (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        path TEXT NOT NULL,
        side TEXT NOT NULL,
        line INTEGER NOT NULL,
        text TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    , rev INTEGER NOT NULL DEFAULT 0, client_request_id TEXT);
CREATE TABLE counters (
        name TEXT PRIMARY KEY,
        value INTEGER NOT NULL
    );
INSERT INTO counters VALUES('run',0);
INSERT INTO counters VALUES('rev',4);
CREATE TABLE attachments (
        session_id TEXT NOT NULL REFERENCES sessions(id),
        id TEXT NOT NULL,
        name TEXT NOT NULL,
        media_type TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        created_at_ms INTEGER NOT NULL,
        -- Where the worker put it in the worktree, once a prompt has carried it.
        placed_at TEXT, by_client INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (session_id, id)
    );
CREATE TABLE hub_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        keep_awake INTEGER NOT NULL DEFAULT 1,
        rev INTEGER NOT NULL DEFAULT 0
    , allow_asked_in_full_access INTEGER NOT NULL DEFAULT 0, github_account_id TEXT REFERENCES github_accounts(id), github_sent TEXT);
INSERT INTO hub_settings VALUES(1,1,2,0,NULL,NULL);
CREATE TABLE accounts (
        id TEXT PRIMARY KEY,
        model_provider TEXT,
        plan TEXT,
        login TEXT,
        email TEXT,
        rev INTEGER NOT NULL DEFAULT 0
    , name TEXT, login_agent TEXT, login_file TEXT, login_connected_at_ms INTEGER, login_ends_at_ms INTEGER, used_at_ms INTEGER);
CREATE TABLE IF NOT EXISTS "events" (
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
        report TEXT,
        rev INTEGER NOT NULL DEFAULT 0, model TEXT, context_tokens INTEGER, id TEXT, lately TEXT,
        PRIMARY KEY (session_id, call)
    );
CREATE TABLE hub_identity (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        hub_id TEXT NOT NULL
    );
INSERT INTO hub_identity VALUES(1,'cc4a7e40ba8a5246e77933a9f7d7a806');
CREATE TABLE instances (
        id TEXT PRIMARY KEY,
        provider TEXT NOT NULL,
        stage TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
CREATE TABLE tools_ports (
        workspace_id TEXT PRIMARY KEY REFERENCES workspaces(id) ON DELETE CASCADE,
        port INTEGER NOT NULL
    );
CREATE TABLE tools_tokens (
        session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
        token TEXT NOT NULL
    );
CREATE TABLE IF NOT EXISTS "new_workspace_draft" (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        draft TEXT,
        rev INTEGER NOT NULL DEFAULT 0
    );
INSERT INTO new_workspace_draft VALUES(1,NULL,3);
CREATE TABLE IF NOT EXISTS "new_workspace_draft_files" (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        media_type TEXT NOT NULL,
        size_bytes INTEGER NOT NULL,
        created_at_ms INTEGER NOT NULL
    );
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
        ended TEXT, watch TEXT,
        CHECK ((ended_at_ms IS NULL) = (ended IS NULL))
    );
CREATE TABLE child_worktrees (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        path TEXT NOT NULL,
        branch TEXT NOT NULL,
        session_id TEXT NOT NULL REFERENCES sessions(id),
        made_by TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        state TEXT NOT NULL,
        removed_at_ms INTEGER,
        branches TEXT,
        ask_key TEXT,
        rev INTEGER NOT NULL DEFAULT 0,
        UNIQUE (session_id, ask_key)
    );
CREATE TABLE runs_under_logins (
        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        run INTEGER NOT NULL,
        account_id TEXT NOT NULL REFERENCES accounts(id),
        PRIMARY KEY (session_id, run)
    );
CREATE TABLE IF NOT EXISTS "account_limits" (
        account_id TEXT NOT NULL,
        window TEXT NOT NULL,
        model TEXT NOT NULL DEFAULT '',
        used_percent INTEGER NOT NULL,
        resets_at_ms INTEGER,
        heard_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, window, model)
    );
CREATE TABLE account_defaults (
        agent TEXT PRIMARY KEY,
        account_id TEXT NOT NULL REFERENCES accounts(id)
    );
CREATE TABLE github_accounts (
        id TEXT PRIMARY KEY,
        user_id INTEGER NOT NULL,
        username TEXT NOT NULL,
        login_kind TEXT,
        login_file TEXT,
        login_connected_at_ms INTEGER,
        login_ends_at_ms INTEGER,
        rev INTEGER NOT NULL DEFAULT 0
    , login_needs_sign_in INTEGER NOT NULL DEFAULT 0, app_id INTEGER, installation_id INTEGER, installed_on_id INTEGER, installed_on TEXT);
CREATE TABLE github_sign_ins (
        id TEXT PRIMARY KEY,
        status TEXT NOT NULL,
        rev INTEGER NOT NULL DEFAULT 0
    );
CREATE TABLE found_github_logins (
        provider TEXT PRIMARY KEY,
        login TEXT NOT NULL,
        rev INTEGER NOT NULL DEFAULT 0
    );
CREATE TABLE devices (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        token_sha256 BLOB NOT NULL UNIQUE,
        paired_at_ms INTEGER NOT NULL,
        last_seen_ms INTEGER NOT NULL
    );
CREATE TABLE github_apps (
        id INTEGER PRIMARY KEY,
        slug TEXT NOT NULL,
        name TEXT NOT NULL,
        owner_id INTEGER NOT NULL,
        owner TEXT NOT NULL,
        bot_id INTEGER,
        bot TEXT,
        key_file TEXT NOT NULL,
        made_from TEXT NOT NULL UNIQUE,
        rev INTEGER NOT NULL DEFAULT 0
    );
CREATE TABLE github_reach (
        repo_id TEXT PRIMARY KEY REFERENCES repos(id),
        reach TEXT NOT NULL,
        answered_at_ms INTEGER,
        stale INTEGER NOT NULL DEFAULT 0,
        rev INTEGER NOT NULL DEFAULT 0
    );
CREATE TABLE github_profiles (
        account_id TEXT PRIMARY KEY REFERENCES github_accounts(id),
        name TEXT,
        email TEXT,
        read_at_ms INTEGER NOT NULL
    );
CREATE TRIGGER repos_rev_on_insert AFTER INSERT ON repos BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE repos SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER repos_rev_on_delete AFTER DELETE ON repos BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
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
CREATE TRIGGER sessions_rev_on_insert AFTER INSERT ON sessions BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER sessions_rev_on_delete AFTER DELETE ON sessions BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
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
CREATE TRIGGER hub_settings_rev_on_insert AFTER INSERT ON hub_settings BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE hub_settings SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
CREATE TRIGGER hub_settings_rev_on_update AFTER UPDATE ON hub_settings WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE hub_settings SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
CREATE TRIGGER accounts_rev_on_insert AFTER INSERT ON accounts BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER accounts_rev_on_update AFTER UPDATE ON accounts WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER subagents_rev_on_insert AFTER INSERT ON subagents BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE subagents SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
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
CREATE TRIGGER new_workspace_draft_rev_on_insert AFTER INSERT ON new_workspace_draft BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE new_workspace_draft SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
CREATE TRIGGER new_workspace_draft_rev_on_update AFTER UPDATE ON new_workspace_draft WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE new_workspace_draft SET rev = (SELECT value FROM counters WHERE name = 'rev');
    END;
CREATE TRIGGER child_worktrees_rev_on_insert AFTER INSERT ON child_worktrees BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE child_worktrees SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER child_worktrees_rev_on_update AFTER UPDATE ON child_worktrees WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE child_worktrees SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER child_worktrees_rev_on_delete AFTER DELETE ON child_worktrees BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms, prompt_cache, draft, decision_draft, untaken, agent_opened, open_approvals, reply_outcome
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER account_limits_rev_on_insert AFTER INSERT ON account_limits BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.account_id;
    END;
CREATE TRIGGER account_limits_rev_on_update AFTER UPDATE OF used_percent, resets_at_ms ON account_limits
        WHEN NEW.used_percent IS NOT OLD.used_percent OR NEW.resets_at_ms IS NOT OLD.resets_at_ms BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.account_id;
    END;
CREATE TRIGGER account_limits_rev_on_delete AFTER DELETE ON account_limits BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = OLD.account_id;
    END;
CREATE TRIGGER waits_move_session_on_observation AFTER UPDATE OF watch ON waits
        WHEN OLD.ended_at_ms IS NULL AND NEW.watch IS NOT OLD.watch
    BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.session_id;
    END;
CREATE TRIGGER sessions_rev_on_agent_capabilities
        AFTER UPDATE OF agent_capabilities ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER account_defaults_rev_on_insert AFTER INSERT ON account_defaults BEGIN
        UPDATE hub_settings SET rev = rev;
    END;
CREATE TRIGGER account_defaults_rev_on_update AFTER UPDATE ON account_defaults
        WHEN NEW.account_id != OLD.account_id BEGIN
        UPDATE hub_settings SET rev = rev;
    END;
CREATE TRIGGER sessions_rev_on_account_source
        AFTER UPDATE OF account_source ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER subagents_rev_on_update AFTER UPDATE ON subagents WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE subagents SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_accounts_rev_on_insert AFTER INSERT ON github_accounts BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_accounts_rev_on_update AFTER UPDATE ON github_accounts WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_accounts SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_sign_ins_rev_on_insert AFTER INSERT ON github_sign_ins BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_sign_ins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_sign_ins_rev_on_update AFTER UPDATE ON github_sign_ins WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_sign_ins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_sign_ins_rev_on_delete AFTER DELETE ON github_sign_ins BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
CREATE TRIGGER found_github_logins_rev_on_insert AFTER INSERT ON found_github_logins BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE found_github_logins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER found_github_logins_rev_on_update AFTER UPDATE ON found_github_logins WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE found_github_logins SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_apps_rev_on_insert AFTER INSERT ON github_apps BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_apps SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_apps_rev_on_update AFTER UPDATE ON github_apps WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_apps SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_apps_rev_on_delete AFTER DELETE ON github_apps BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;
CREATE TRIGGER github_reach_rev_on_insert AFTER INSERT ON github_reach BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_reach SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER github_reach_rev_on_update AFTER UPDATE ON github_reach WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE github_reach SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE TRIGGER repos_rev_on_update AFTER UPDATE OF name, path, url, created_at_ms ON repos
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE repos SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
CREATE INDEX sessions_by_workspace ON sessions (workspace_id, created_at_ms);
CREATE INDEX diff_comments_by_workspace ON diff_comments (workspace_id, created_at_ms);
CREATE UNIQUE INDEX workspaces_by_request ON workspaces (client_request_id) WHERE client_request_id IS NOT NULL;
CREATE UNIQUE INDEX sessions_by_request ON sessions (client_request_id) WHERE client_request_id IS NOT NULL;
CREATE UNIQUE INDEX diff_comments_by_request ON diff_comments (client_request_id)
        WHERE client_request_id IS NOT NULL;
CREATE UNIQUE INDEX events_client_msg ON events (session_id, client_msg_id) WHERE client_msg_id IS NOT NULL;
CREATE INDEX events_by_kind ON events (session_id, kind);
CREATE INDEX events_by_subagent ON events (session_id, subagent, place);
CREATE INDEX waits_workspace ON waits (workspace_id, ended_at_ms);
CREATE INDEX waits_open ON waits (workspace_id) WHERE ended_at_ms IS NULL;
CREATE INDEX waits_session ON waits (session_id, call);
CREATE UNIQUE INDEX child_worktrees_there ON child_worktrees (path) WHERE state != 'removed';
CREATE INDEX child_worktrees_of ON child_worktrees (workspace_id);
CREATE UNIQUE INDEX subagents_by_id ON subagents (session_id, id);
COMMIT;
PRAGMA user_version = 71;
