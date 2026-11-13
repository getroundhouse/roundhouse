//! The migration that puts on each session's record how its newest turn ended, kept apart since it's long. It has its
//! place in `MIGRATIONS`.

/// How the turn that ended at each session's `reply_seq` ended, as its end said, which a Session record shows, so the
/// trigger names it. Filled in from the log for what came before, after the trigger, so each session it fills in gets a
/// newer revision and a client's next list takes it. A session's reply is one event, found by its key, so filling them
/// in reads one row of the log for each session rather than searching it. A reply that isn't a turn's end, as when the
/// agent said it was idle without one, said nothing of how the turn ended.
pub(super) const REPLY_OUTCOME: &str = r"
    ALTER TABLE sessions ADD COLUMN reply_outcome TEXT;
    DROP TRIGGER sessions_rev_on_update;
    CREATE TRIGGER sessions_rev_on_update
        AFTER UPDATE OF workspace_id, agent, model, permission_mode, status, title, closed_at_ms, reply_seq, read_seq,
            background_tasks, title_source, queued, queue_held, turn_started_at_ms, effort, context, account_id,
            user_active_at_ms, prompt_cache, draft, answer_draft, untaken, agent_opened, open_approvals, reply_outcome
        ON sessions
        WHEN NEW.rev = OLD.rev BEGIN
        UPDATE counters SET value = value + 1 WHERE name = 'rev';
        UPDATE sessions SET rev = (SELECT value FROM counters WHERE name = 'rev') WHERE rowid = NEW.rowid;
    END;
    UPDATE sessions SET reply_outcome = (
        SELECT ended.body ->> '$.outcome' FROM events AS ended
            WHERE ended.session_id = sessions.id AND ended.seq = sessions.reply_seq AND ended.kind = 'turn_ended')
        WHERE EXISTS (
            SELECT 1 FROM events AS ended
                WHERE ended.session_id = sessions.id AND ended.seq = sessions.reply_seq AND ended.kind = 'turn_ended');
";
