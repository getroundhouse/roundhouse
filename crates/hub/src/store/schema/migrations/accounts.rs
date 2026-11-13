//! The migrations that keep the accounts agents run under and what's heard of their limits, kept apart since
//! they're long. Each has its place in `MIGRATIONS`.

/// What agents have heard of each account's limits, one row for each window of each account, with when it was
/// heard. It's not a record the feed announces: a client reads it when it shows it. Accounts became records later.
pub(super) const LIMITS: &str = r"
    CREATE TABLE account_limits (
        account_id TEXT NOT NULL,
        window TEXT NOT NULL,
        used_percent INTEGER NOT NULL,
        resets_at_ms INTEGER,
        heard_at_ms INTEGER NOT NULL,
        PRIMARY KEY (account_id, window)
    );
";

/// Each account agents run under is a record the feed announces, so the hub keeps who it is beside what its agents
/// have heard of its limits. `heard_at_ms` is when an agent under it last heard of any, and changes whenever what's
/// kept of them does, which is what gives the record a newer revision. Every account whose limits were kept gets
/// one, without who it is until an agent under it next launches. Each session names the account its agent's newest
/// launch runs under, which a Session record shows, so the trigger names it.
pub(super) const ACCOUNTS: &str = r"
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
";
