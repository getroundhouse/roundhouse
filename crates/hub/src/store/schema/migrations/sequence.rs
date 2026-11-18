use super::super::migration::Migration::{self, Rust, Sql};
use super::{
    accounts, activity, cache, comments, drafts, first, github_accounts, github_sign_ins, hub_settings, limits, logins,
    providers, replies, repos, subagents, tools, waiting, waits, workers, workspaces,
};

/// Applied in order. `PRAGMA user_version` records how many have run.
pub(in super::super) const MIGRATIONS: &[Migration] = &[
    // What the hub began with.
    Sql(first::FIRST),
    // Comments on each workspace's diff, in `comments`.
    Sql(comments::DIFF_COMMENTS),
    Sql(r#"
    ALTER TABLE workspaces ADD COLUMN setup TEXT NOT NULL DEFAULT '{"state":"not_started"}';
    ALTER TABLE workspaces ADD COLUMN port_base INTEGER NOT NULL DEFAULT 0;
"#),
    Sql(r"
    ALTER TABLE sessions ADD COLUMN title TEXT;
    ALTER TABLE sessions ADD COLUMN closed_at_ms INTEGER;
"),
    // Rebuilt rather than altered, so `run` comes before `raw` and `raw` stays last.
    Sql(r"
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
"),
    // Each record's revision is set by triggers, so no write can forget it. A delete moves the counter too,
    // so a client can tell a removal from what came before it. Revisions start at 1, because 0 means a hub
    // that doesn't keep them.
    Sql(r"
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
"),
    // What a client chose for a request that created a record, so sending it again returns that record.
    Sql(r"
    ALTER TABLE workspaces ADD COLUMN client_request_id TEXT;
    ALTER TABLE sessions ADD COLUMN client_request_id TEXT;
    ALTER TABLE diff_comments ADD COLUMN client_request_id TEXT;
    CREATE UNIQUE INDEX workspaces_by_request ON workspaces (client_request_id) WHERE client_request_id IS NOT NULL;
    CREATE UNIQUE INDEX sessions_by_request ON sessions (client_request_id) WHERE client_request_id IS NOT NULL;
    CREATE UNIQUE INDEX diff_comments_by_request ON diff_comments (client_request_id)
        WHERE client_request_id IS NOT NULL;
"),
    // Each session's newest reply, and how far it's been read. A session from before this has no reply, so nothing
    // turns unread by updating. Both are columns a Session record shows, so the trigger names them.
    Sql(r"
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
"),
    // What each session's agent is running in the background, as JSON. A session from before this had nothing
    // running that's still going, since a new hub's agents are all new. The Session record shows it, so the trigger
    // names it.
    Sql(r"
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
"),
    // Titles, and where each came from. A workspace from before takes its oldest chat's title, which is that chat's
    // first prompt's first line, so whatever is said in it next doesn't become its title. Where a session's title
    // came from is a column its record shows, so the trigger names it.
    Sql(r"
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
"),
    // The files a session's prompts carry. What each is never changes once added, and where it was put is filled in
    // once. Each prompt's event says which it carries.
    // Their bytes are files in the attachments directory, so copying the database before a migration stays quick.
    Sql(r"
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
"),
    // Each session's queue, as a JSON list, and whether it's held. A Session record shows both, so the trigger names
    // them too. Also the prompts its agent hasn't taken or dropped yet, as a JSON list of where they're recorded, so a
    // hub that starts again can drop those its last one's agents took with them. Only the hub reads that, so the
    // trigger leaves it out.
    Sql(r"
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
"),
    // When each session's newest turn started. A session from before this has none until its next turn. It's a column
    // a Session record shows, so the trigger names it.
    Sql(r"
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
"),
    // How hard each session's agent thinks, as the CLI names the level. A session from before this has none, which
    // leaves it to the user's own settings, as it always did. A Session record shows it, so the trigger names it.
    Sql(r"
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
"),
    // What the hub last found of each workspace's pull request, in `workspaces`.
    Sql(workspaces::PULL_REQUEST),
    // How full each session's agent's context is, as its newest report said, as JSON. A session from before this has
    // none until its agent next says. A Session record shows it, so the trigger names it.
    Sql(r"
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
"),
    // What agents have heard of each account's limits, in `accounts`.
    Sql(accounts::LIMITS),
    // The hub's own settings, in `hub_settings`.
    Sql(hub_settings::HUB_SETTINGS),
    // Every session names its model and effort from here on, so the agent's CLI never picks either for itself, and a
    // session that doesn't won't read. One that left either to the CLI gets what the hub gives a new session that names
    // neither. A model names none as `ModelName` says: empty, `default`, which was the picker's name for the CLI's own
    // choice, or with anything but printable ASCII, which the GLOB's range matches.
    Sql(r"
    UPDATE sessions SET model = 'opus'
        WHERE model IS NULL OR model = '' OR model = 'default' OR model GLOB '*[^!-~]*';
    UPDATE sessions SET effort = 'xhigh' WHERE effort IS NULL;
"),
    // Each account agents run under, as a record the feed announces, in `accounts`.
    Sql(accounts::ACCOUNTS),
    // A chat closed before anything was sent in it is deleted from here on, since there's nothing to reopen, so the
    // ones closed that way before go too. One with a file added is left: its bytes are outside the database, where
    // only the hub can let them go. Deleting moves the revision, so a client's next list drops them.
    Sql(r"
    DELETE FROM sessions
        WHERE closed_at_ms IS NOT NULL AND queued = '[]'
            AND NOT EXISTS (SELECT 1 FROM events WHERE events.session_id = sessions.id)
            AND NOT EXISTS (SELECT 1 FROM attachments WHERE attachments.session_id = sessions.id);
"),
    // The subagent each event is from, and its place among those from the same one. In `subagents`, as are the other
    // migrations that record subagents.
    Sql(subagents::EVENTS_BY_SUBAGENT),
    // Each subagent's record, filled in for those from before it from what the log kept.
    Sql(subagents::RECORDS),
    // Every session names its approval mode from here on, and every agent has full access unless it's planning. One
    // that left it to the user's own Claude settings, or chose a mode only the old picker offered, such as asking first,
    // gets full access.
    Sql(r"
    UPDATE sessions SET permission_mode = 'bypassPermissions'
        WHERE permission_mode IS NULL OR permission_mode NOT IN ('plan', 'bypassPermissions');
"),
    // What each subagent has done lately, which its record shows while it works.
    Sql(subagents::RECENT_WORK),
    // When the person last sent each session's agent something, and last did something in each workspace, in
    // `activity`, since they're long.
    Sql(activity::BY_SESSION),
    Sql(activity::BY_WORKSPACE),
    // What the API's cache holds of each session's conversation, in `cache`.
    Sql(cache::PROMPT_CACHE),
    // What's typed and attached in each session's composer and not sent, and which files a client added, in `drafts`.
    Sql(drafts::DRAFT),
    Sql(drafts::ADDED_BY_CLIENT),
    // Who this hub is, and each instance it asks a provider for, in `providers`.
    Sql(providers::HUB_IDENTITY),
    Sql(providers::INSTANCES),
    // The model each subagent runs on, which its record shows.
    Sql(subagents::MODELS),
    // The port each workspace's worker last served its tools on, and each session's token for them, in `tools`.
    Sql(tools::PORTS_AND_TOKENS),
    // What's chosen and typed in answer to each session's question, and the comment being written on each workspace's
    // diff, and not sent.
    Sql(drafts::ANSWER_DRAFT),
    Sql(drafts::COMMENT_DRAFT),
    // What's in the new-workspace dialog and not made yet, with its files.
    Sql(drafts::WORKSPACE_DRAFT),
    // What each workspace's worker last read of its worktree's branches, in `workspaces`.
    Sql(workspaces::BRANCHES),
    // What each session last recorded of what its worker numbered, in `workers`.
    Sql(workers::RECORDED_FROM_WORKER),
    // Each agent's waits on its workspace's pull request, and the triggers that have a session's record show the one it
    // has open, in `waits`.
    Sql(waits::WAITS),
    Sql(waits::SESSIONS_SHOW_WAITS),
    // The approvals each session's agent is waiting on, which its record shows with what it has yet to take and whether
    // it has opened its conversation, in `waiting`.
    Sql(waiting::WAITING_ON),
    // Which build each workspace's worker is, while it's another than the hub's, in `workspaces`.
    Sql(workspaces::WORKER_BUILD),
    // The new-workspace dialog's tables, named for it.
    Sql(drafts::NEW_WORKSPACE_DRAFT),
    // About how many tokens each subagent's conversation holds, which its record shows.
    Sql(subagents::CONTEXT_TOKENS),
    // The logins the hub holds for agents' accounts, on the accounts' rows, in `logins`.
    Sql(logins::HELD),
    // Each workspace's other worktrees, made for its work, in `workspaces`.
    Sql(workspaces::CHILD_WORKTREES),
    // How each session's newest turn ended, which its record shows, in `replies`.
    Sql(replies::REPLY_OUTCOME),
    // The account whose login the hub gave each run it launched under one, in `logins`.
    Sql(logins::RUNS),
    // Whether the hub allows a tool's use itself when an agent with full access asks anyway, in `hub_settings`.
    Sql(hub_settings::ALLOW_ASKED_IN_FULL_ACCESS),
    // Each model's own week of an account's limits, kept apart by the model, in `limits`.
    Sql(limits::BY_MODEL),
    // When the person last sent something to an agent under each account, in `accounts`.
    Sql(accounts::USED_AT),
    Sql(waits::REMOTE_WAITS),
    // A run reports its conversation identity and claims in one durable initialization.
    Sql(super::agent_runs::AGENT_RUNS),
    Sql(super::account_defaults::ACCOUNT_DEFAULTS),
    // The account each chat is on, whose login its launches take, in `session_accounts`.
    Sql(super::session_accounts::ASSIGNED_ACCOUNTS),
    // Where each chat's login comes from, the hub or ambient, in place of the account it was on, in `session_accounts`.
    Sql(super::session_accounts::ACCOUNT_SOURCES),
    // Each subagent's ID, apart from the call that started it.
    Sql(subagents::IDS),
    // The GitHub accounts the hub holds a login for, and which is the default, in `github_accounts`.
    Sql(github_accounts::GITHUB_ACCOUNTS),
    // Each sign-in with GitHub, and whether a GitHub account's login needs signing in again, in `github_sign_ins`.
    Sql(github_sign_ins::GITHUB_SIGN_INS),
    // Who each provider's machines' own `gh` is logged in as, found since the hub started, in `github_accounts`.
    Sql(github_accounts::FOUND_GITHUB_LOGINS),
    Sql(super::devices::DEVICES),
    // Each app of your own, and which GitHub accounts are its installations, in `github_accounts`.
    Sql(github_accounts::GITHUB_APPS),
    // Whether the default GitHub account can push to each repository, in `github_accounts`.
    Sql(github_accounts::GITHUB_REACH),
    // What GitHub's profile of each GitHub account's user says, which its commit identity is made from, in
    // `github_accounts`.
    Sql(github_accounts::GITHUB_PROFILES),
    // The address each repository's checkout fetches from, and whether it's to be read, in `repos`.
    Sql(repos::URLS),
    // What the hub sends every worker for the default GitHub account, on the hub's settings, in `github_accounts`.
    Sql(github_accounts::GITHUB_SENT),
    // Whether a push from each repository's checkout goes round the git credential, in `repos`.
    Sql(repos::PUSH_ROUTES),
    // What each subagent has done lately, with each call typed, which its record shows while it works.
    Sql(subagents::LATELY),
    // Typing what Claude's adapter recorded before it typed its calls: each call, with what came back for it, the call
    // each request for approval asked to make, and what each working subagent has done lately, in `typed_calls`.
    Rust(super::typed_calls::typed_calls),
    // Typing each request for approval, what closed it, with the decision it stood for, and the draft of an answer to
    // one, which `sessions.decision_draft` holds now, in `typed_requests`.
    Rust(super::typed_requests::typed_requests),
    // Typing each chat's approval mode, which `sessions.mode` holds now, each one its log names, and the
    // new-workspace dialog's, in `typed_modes`.
    Rust(super::typed_modes::typed_modes),
];
