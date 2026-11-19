//! The migration for the proposals agents make, which has its place in `MIGRATIONS`.

/// Each proposal an agent made, with a `rev` like every record's, and each of its revisions, as the agent wrote
/// them. `ask_key` is the worker's key for the ask that made the proposal, unique within the session that asked, so
/// asking again makes it once. A revision's `number` counts from 1. `proposed` is what the agent wrote, as JSON, and
/// `call` the agent's call that wrote it, NULL when its CLI didn't say. `response` is the person's response, as JSON,
/// NULL while it waits for one, and `response_request` the `client_request_id` of the request that gave it, so the same
/// request sent again is known for the same answer, as a comment's is. Writing a revision moves its proposal's `rev`,
/// since a client has the newest as part of the proposal.
pub(super) const PROPOSALS: &str = r"
    CREATE TABLE proposals (
        id TEXT PRIMARY KEY,
        workspace_id TEXT NOT NULL REFERENCES workspaces(id),
        session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
        ask_key TEXT NOT NULL,
        created_at_ms INTEGER NOT NULL,
        rev INTEGER NOT NULL DEFAULT 0,
        UNIQUE (session_id, ask_key)
    );
    CREATE TRIGGER proposals_rev_on_insert AFTER INSERT ON proposals BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE proposals SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER proposals_rev_on_update AFTER UPDATE ON proposals WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE proposals SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    CREATE TRIGGER proposals_rev_on_delete AFTER DELETE ON proposals BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
    END;

    CREATE TABLE proposal_revisions (
        proposal_id TEXT NOT NULL REFERENCES proposals(id) ON DELETE CASCADE,
        number INTEGER NOT NULL,
        call TEXT,
        proposed TEXT NOT NULL,
        made_at_ms INTEGER NOT NULL,
        response TEXT,
        response_request TEXT,
        PRIMARY KEY (proposal_id, number)
    );
    CREATE TRIGGER proposal_revisions_rev_on_insert AFTER INSERT ON proposal_revisions BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE proposals SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.proposal_id;
    END;
    CREATE TRIGGER proposal_revisions_rev_on_update AFTER UPDATE ON proposal_revisions BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE proposals SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE id = NEW.proposal_id;
    END;
";

/// The images what the agent wrote in each revision points to in its worktree, as JSON, each by the address it gives it
/// with the attachment of the chat that keeps it, as a reply's text block names them. NULL when it points to none, as
/// every revision written before did.
pub(super) const REVISION_IMAGES: &str = r"
    ALTER TABLE proposal_revisions ADD COLUMN images TEXT;
";

/// What the agent said changed in each revision after the first, in Markdown, NULL for the first, and the worker's key
/// for the ask that wrote it, unique within its proposal, so asking again writes it once. NULL for the first, which
/// `proposals.ask_key` keys, and for every revision written before.
pub(super) const REVISIONS: &str = r"
    ALTER TABLE proposal_revisions ADD COLUMN changed TEXT;
    ALTER TABLE proposal_revisions ADD COLUMN ask_key TEXT;
    CREATE UNIQUE INDEX proposal_revisions_by_ask ON proposal_revisions (proposal_id, ask_key) WHERE ask_key IS NOT NULL;
";

/// The agent's withdrawal of each proposal it withdrew, as JSON: why, the call that withdrew it and when. NULL for one
/// it hasn't, as every proposal made before was.
pub(super) const WITHDRAWALS: &str = r"
    ALTER TABLE proposals ADD COLUMN withdrawn TEXT;
";
