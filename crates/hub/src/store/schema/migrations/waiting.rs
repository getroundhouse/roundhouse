//! The migration that puts on each session's record what its agent is waiting on and has yet to take, kept apart since
//! it's long. It has its place in `MIGRATIONS`.

/// The approvals each session's agent is waiting on, as a JSON list of each request's `seq` and approval ID, oldest
/// first, filled in from the log: every approval with a request that no later event resolved, at the first such
/// request. A Session record shows them, and the prompts its agent has yet to take, which were only the hub's before,
/// and whether the agent has opened its conversation, so the trigger names all three.
pub(super) const WAITING_ON: &str = r"
    ALTER TABLE sessions ADD COLUMN open_approvals TEXT NOT NULL DEFAULT '[]';
    UPDATE sessions SET open_approvals = (
        SELECT ifnull(json_group_array(json_object('seq', seq, 'approval_id', approval_id)), '[]') FROM (
            SELECT min(requested.seq) AS seq, json_extract(requested.body, '$.approval_id') AS approval_id
            FROM events AS requested
            WHERE requested.session_id = sessions.id AND requested.kind = 'approval_requested'
                AND NOT EXISTS (
                    SELECT 1 FROM events AS resolved
                    WHERE resolved.session_id = requested.session_id AND resolved.kind = 'approval_resolved'
                        AND resolved.seq > requested.seq
                        AND json_extract(resolved.body, '$.approval_id') = json_extract(requested.body, '$.approval_id'))
            GROUP BY approval_id
            ORDER BY seq));
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms, prompt_cache, draft, answer_draft, untaken, agent_opened, open_approvals
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
";
