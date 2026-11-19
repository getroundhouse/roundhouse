use super::super::made::Made;
use super::super::queue::{Queued, queue_on};
use super::super::{Store, StoreError, current_rev, json_at, lock, rev_of};
use protocol::attachment::Attachment;
use protocol::model::{
    AnswerProblem, AskKey, Choice, ClientMessageId, ClientRequestId, Proposal, ProposalId, ProposalRevision,
    QueuedPrompt, Response, Rev, RevisionNumber, SessionId, ToolUseId,
};
use protocol::prompt::{Prompt, PromptFrom};
use rusqlite::types::Type;
use rusqlite::{Connection, OptionalExtension, Row, params};

/// A proposal with its newest revision, read from `proposals` as `p` and `proposal_revisions` as `r`.
const PROPOSAL_COLUMNS: &str = "p.id, p.workspace_id, p.session_id, p.created_at_ms, p.rev, r.number, r.call, r.proposed, r.made_at_ms, r.response";
const PROPOSAL_WITH_NEWEST: &str = "proposals p JOIN proposal_revisions r ON r.proposal_id = p.id
    AND r.number = (SELECT MAX(number) FROM proposal_revisions WHERE proposal_id = p.id)";
const REVISION_COLUMNS: &str = "number, call, proposed, made_at_ms, response";

/// What answering a proposal did.
pub enum Answering {
    /// It's answered now, and the prompt that tells its agent is queued.
    Kept { proposal: Proposal, queued: Queued },
    /// The same request answered it before, so nothing changed, and nothing more is queued.
    Again(Proposal),
    /// Another request answered it, so it's left as it was.
    Otherwise(Proposal),
    /// The agent has revised it since the revision answered, so it's left as it was.
    Revised(Proposal),
    /// The answer doesn't fit the revision, as [`Proposed::check_answer`](protocol::model::Proposed::check_answer)
    /// says, so it's left as it was.
    Refused(AnswerProblem),
}

/// The answer the hub keeps for a proposal's revision, from a request, with the prompt that tells its agent.
pub struct AnswerToKeep<'a> {
    /// The revision the person answered.
    pub revision: RevisionNumber,
    pub choice: &'a Choice,
    /// The request that gave it, which the same request sent again is known by.
    pub request: &'a ClientRequestId,
    /// The ID the hub gave the prompt, which keeps it from queued to recorded.
    pub message: ClientMessageId,
    /// What the prompt says, written from the proposal as the commit that keeps the answer reads it.
    pub told: &'a dyn Fn(&Proposal, &Choice) -> Prompt<Attachment>,
}

/// The revision in `REVISION_COLUMNS` from `at` on.
fn revision_from_row(row: &Row<'_>, at: usize) -> rusqlite::Result<ProposalRevision> {
    // Only this build and newer ones write it, and a newer one's fields that this build doesn't know are left out.
    let proposed = json_at(row, at + 2)?
        .ok_or_else(|| rusqlite::Error::InvalidColumnType(at + 2, "proposed".into(), Type::Text))?;
    Ok(ProposalRevision {
        number: RevisionNumber(row.get(at)?),
        call: row.get::<_, Option<String>>(at + 1)?.map(Into::into),
        proposed,
        made_at_ms: row.get(at + 3)?,
        // A response written in a form this build can't read, by a newer one, is still a response, so the revision
        // isn't taken for waiting, and answered again over it.
        response: row
            .get::<_, Option<String>>(at + 4)?
            .map(|json| serde_json::from_str(&json).unwrap_or(Response::Unknown)),
    })
}

fn proposal_from_row(row: &Row<'_>) -> rusqlite::Result<Proposal> {
    Ok(Proposal {
        id: row.get::<_, String>(0)?.into(),
        workspace_id: row.get::<_, String>(1)?.into(),
        session_id: row.get::<_, String>(2)?.into(),
        current: revision_from_row(row, 5)?,
        created_at_ms: row.get(3)?,
        rev: rev_of(row, 4)?,
    })
}

fn proposal_on(conn: &Connection, id: &ProposalId) -> Result<Proposal, StoreError> {
    conn.query_row(
        &format!("SELECT {PROPOSAL_COLUMNS} FROM {PROPOSAL_WITH_NEWEST} WHERE p.id = ?"),
        [id.as_str()],
        proposal_from_row,
    )
    .optional()?
    .ok_or_else(|| StoreError::NotFound { kind: "proposal", id: id.to_string() })
}

/// Takes back the answer to the revision `revision` of the proposal `id`, on `conn`, as the prompt that would tell its
/// agent is taken out of the queue in the same commit, so it waits for the person again, and the request that answered
/// it can answer it afresh.
pub(in super::super) fn take_back_answer_on(
    conn: &Connection,
    id: &ProposalId,
    revision: RevisionNumber,
) -> Result<Proposal, StoreError> {
    conn.execute(
        "UPDATE proposal_revisions SET response = NULL, response_request = NULL WHERE proposal_id = ? AND number = ?",
        params![id.as_str(), revision.0],
    )?;
    proposal_on(conn, id)
}

impl Store {
    /// Keeps `proposal`, with its revision, unless its session's ask with `key` made one already, which it returns
    /// instead.
    pub fn insert_proposal(&self, proposal: &Proposal, key: &AskKey) -> Result<Made<Proposal>, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let earlier = tx
            .query_row(
                "SELECT id FROM proposals WHERE session_id = ? AND ask_key = ?",
                params![proposal.session_id.as_str(), key.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(earlier) = earlier {
            return Ok(Made::Earlier(proposal_on(&tx, &earlier.into())?));
        }
        tx.execute(
            "INSERT INTO proposals (id, workspace_id, session_id, ask_key, created_at_ms) VALUES (?, ?, ?, ?, ?)",
            params![
                proposal.id.as_str(),
                proposal.workspace_id.as_str(),
                proposal.session_id.as_str(),
                key.as_str(),
                proposal.created_at_ms,
            ],
        )?;
        let first = &proposal.current;
        tx.execute(
            "INSERT INTO proposal_revisions (proposal_id, number, call, proposed, made_at_ms) VALUES (?, ?, ?, ?, ?)",
            params![
                proposal.id.as_str(),
                first.number.0,
                first.call.as_ref().map(ToolUseId::as_str),
                serde_json::to_string(&first.proposed)?,
                first.made_at_ms,
            ],
        )?;
        let made = proposal_on(&tx, &proposal.id)?;
        tx.commit()?;
        Ok(Made::New(made))
    }

    /// The session's proposals, oldest first, each with its newest revision, read in one transaction with the revision
    /// they're as of.
    pub fn list_proposals(&self, session_id: &SessionId) -> Result<(Vec<Proposal>, Rev), StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let proposals = tx
            .prepare_cached(&format!(
                "SELECT {PROPOSAL_COLUMNS} FROM {PROPOSAL_WITH_NEWEST}
                     WHERE p.session_id = ? ORDER BY p.created_at_ms, p.rowid"
            ))?
            .query_map([session_id.as_str()], proposal_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let rev = current_rev(&tx)?;
        tx.commit()?;
        Ok((proposals, rev))
    }

    /// Keeps the answer to the revision of the proposal `id` that `answer` names, and puts the prompt that tells its
    /// agent in its chat's queue, written by the hub for that revision, both or neither, so no answer is kept without
    /// the prompt that tells its agent. Answered already by the same request, nothing changes and nothing more is
    /// queued, since the first did both. Answered by another, or revised since the revision it names, nothing changes
    /// either, and the proposal is returned as it is. The answer is checked against the revision in the same commit,
    /// so one checked against a copy read before isn't kept once that copy's out of date, such as when an answer is
    /// taken back.
    pub fn answer_proposal(&self, id: &ProposalId, answer: AnswerToKeep<'_>) -> Result<Answering, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let earlier = proposal_on(&tx, id)?;
        let by: Option<String> = tx
            .query_row(
                "SELECT response_request FROM proposal_revisions WHERE proposal_id = ? AND number = ?",
                params![id.as_str(), answer.revision.0],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound {
                kind: "proposal revision",
                id: format!("{id} {}", answer.revision),
            })?;
        if by.as_deref() == Some(answer.request.as_str()) {
            return Ok(Answering::Again(earlier));
        }
        if answer.revision != earlier.current.number {
            return Ok(Answering::Revised(earlier));
        }
        if earlier.current.response.is_some() {
            return Ok(Answering::Otherwise(earlier));
        }
        if let Err(problem) = earlier.current.proposed.check_answer(&answer.choice.chosen, &answer.choice.note) {
            return Ok(Answering::Refused(problem));
        }
        let told = (answer.told)(&earlier, answer.choice);
        tx.execute(
            "UPDATE proposal_revisions SET response = ?, response_request = ? WHERE proposal_id = ? AND number = ?",
            params![
                serde_json::to_string(&Response::Chose(answer.choice.clone()))?,
                answer.request.as_str(),
                id.as_str(),
                answer.revision.0
            ],
        )?;
        let proposal = proposal_on(&tx, id)?;
        let from = PromptFrom::ProposalResponse { proposal: id.clone(), revision: answer.revision };
        let prompt = QueuedPrompt { client_msg_id: answer.message, prompt: told, from };
        let queued = queue_on(&tx, &proposal.session_id, &prompt)?;
        tx.commit()?;
        Ok(Answering::Kept { proposal, queued })
    }

    pub fn get_proposal(&self, id: &ProposalId) -> Result<Proposal, StoreError> {
        proposal_on(&lock(&self.reader), id)
    }

    /// The revisions of the proposal `id` from `from` on, oldest first, at most `limit` of them, read in one
    /// transaction with the number of its newest.
    pub fn proposal_revisions(
        &self,
        id: &ProposalId,
        from: RevisionNumber,
        limit: u32,
    ) -> Result<(Vec<ProposalRevision>, RevisionNumber), StoreError> {
        let mut conn = lock(&self.reader);
        let tx = conn.transaction()?;
        let newest = proposal_on(&tx, id)?.current.number;
        let revisions = tx
            .prepare_cached(&format!(
                "SELECT {REVISION_COLUMNS} FROM proposal_revisions
                     WHERE proposal_id = ? AND number >= ? ORDER BY number LIMIT ?"
            ))?
            .query_map(params![id.as_str(), from.0, limit], |row| revision_from_row(row, 0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tx.commit()?;
        Ok((revisions, newest))
    }
}
