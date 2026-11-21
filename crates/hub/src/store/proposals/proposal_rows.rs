use super::super::made::Made;
use super::super::queue::{Queued, queue_on};
use super::super::{Columns, Store, StoreError, current_rev, lock};
use super::proposal_reads::{
    PROPOSAL_WITH_NEWEST, RevisionColumns, proposal_columns, proposal_from_row, proposal_on, revision_from_row,
    with_summaries,
};
use protocol::attachment::Attachment;
use protocol::event::LinkedImage;
use protocol::model::{
    AnswerProblem, AskKey, ClientMessageId, ClientRequestId, Proposal, ProposalId, ProposalRevision, Proposed,
    QueuedPrompt, Response, Rev, RevisionNumber, RevisionStanding, SessionId, ToolUseId, Withdrawal,
};
use protocol::prompt::{Prompt, PromptFrom};
use rusqlite::{Connection, OptionalExtension, params};

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
    /// The agent has withdrawn it, so it's left as it was.
    Withdrawn(Proposal),
    /// The answer doesn't fit the revision, as [`Proposed::check_response`](protocol::model::Proposed::check_response)
    /// says, so it's left as it was.
    Refused(AnswerProblem),
}

/// The answer the hub keeps for a proposal's revision, from a request, with the prompt that tells its agent: a choice,
/// or a request for another round.
pub struct AnswerToKeep<'a> {
    /// The revision the person answered.
    pub revision: RevisionNumber,
    pub response: &'a Response,
    /// The request that gave it, which the same request sent again is known by.
    pub request: &'a ClientRequestId,
    /// The ID the hub gave the prompt, which keeps it from queued to recorded.
    pub message: ClientMessageId,
    /// What the prompt says, written from the proposal as the commit that keeps the answer reads it.
    pub told: &'a dyn Fn(&Proposal, &Response) -> Prompt<Attachment>,
}

/// What revising a proposal did.
pub enum Revising {
    /// Wrote the revision, or found the one the same ask wrote before.
    Wrote(Made<Proposal>),
    /// The agent has withdrawn it, so it's left as it was.
    Withdrawn(Proposal),
}

/// What withdrawing a proposal did.
pub enum Withdrawing {
    /// It's withdrawn now.
    Withdrew(Proposal),
    /// It was withdrawn already, so it's left as it was.
    Already(Proposal),
    /// The person has responded to its newest revision in a way that settles it, by choosing, so it's left as it was.
    Settled(Proposal),
}

/// A revision the agent wrote of a proposal it made, which the store numbers, one past the newest.
pub struct NewRevision {
    pub changed: String,
    pub call: Option<ToolUseId>,
    pub proposed: Proposed,
    pub made_at_ms: i64,
    pub images: Vec<LinkedImage>,
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
    /// The proposal its session's ask with `key` made, if one did.
    pub fn proposal_asked(&self, session_id: &SessionId, key: &AskKey) -> Result<Option<Proposal>, StoreError> {
        let conn = lock(&self.reader);
        let made = conn
            .query_row(
                "SELECT id FROM proposals WHERE session_id = ? AND ask_key = ?",
                params![session_id.as_str(), key.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        made.map(|id| proposal_on(&conn, &id.into())).transpose()
    }

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
            "INSERT INTO proposal_revisions (proposal_id, number, call, proposed, made_at_ms, images)
                 VALUES (?, ?, ?, ?, ?, ?)",
            params![
                proposal.id.as_str(),
                first.number.0,
                first.call.as_ref().map(ToolUseId::as_str),
                serde_json::to_string(&first.proposed)?,
                first.made_at_ms,
                (!first.images.is_empty()).then(|| serde_json::to_string(&first.images)).transpose()?,
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
                "SELECT {} FROM {PROPOSAL_WITH_NEWEST}
                     WHERE p.session_id = ? ORDER BY p.created_at_ms, p.rowid",
                proposal_columns()
            ))?
            .query_map([session_id.as_str()], proposal_from_row)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let proposals =
            proposals.into_iter().map(|proposal| with_summaries(&tx, proposal)).collect::<Result<_, _>>()?;
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
        if earlier.withdrawn.is_some() {
            return Ok(Answering::Withdrawn(earlier));
        }
        if earlier.current.response.is_some() {
            return Ok(Answering::Otherwise(earlier));
        }
        if let Err(problem) = earlier.current.proposed.check_response(answer.response) {
            return Ok(Answering::Refused(problem));
        }
        let told = (answer.told)(&earlier, answer.response);
        tx.execute(
            "UPDATE proposal_revisions SET response = ?, response_request = ? WHERE proposal_id = ? AND number = ?",
            params![serde_json::to_string(answer.response)?, answer.request.as_str(), id.as_str(), answer.revision.0],
        )?;
        let proposal = proposal_on(&tx, id)?;
        let from = PromptFrom::ProposalResponse { proposal: id.clone(), revision: answer.revision };
        let prompt = QueuedPrompt { client_msg_id: answer.message, prompt: told, from };
        let queued = queue_on(&tx, &proposal.session_id, &prompt)?;
        tx.commit()?;
        Ok(Answering::Kept { proposal, queued })
    }

    /// Writes `revision` as the newest of the proposal `id`, numbered one past the newest it has, unless its session's
    /// ask with `key` wrote one already, which leaves the proposal as it is, or the agent has withdrawn it.
    pub fn revise_proposal(
        &self,
        id: &ProposalId,
        revision: &NewRevision,
        key: &AskKey,
    ) -> Result<Revising, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let earlier = proposal_on(&tx, id)?;
        let asked: Option<u32> = tx
            .query_row(
                "SELECT number FROM proposal_revisions WHERE proposal_id = ? AND ask_key = ?",
                params![id.as_str(), key.as_str()],
                |row| row.get(0),
            )
            .optional()?;
        if asked.is_some() {
            return Ok(Revising::Wrote(Made::Earlier(earlier)));
        }
        if earlier.withdrawn.is_some() {
            return Ok(Revising::Withdrawn(earlier));
        }
        tx.execute(
            "INSERT INTO proposal_revisions (proposal_id, number, call, proposed, made_at_ms, images, changed, ask_key)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id.as_str(),
                earlier.current.number.next().0,
                revision.call.as_ref().map(ToolUseId::as_str),
                serde_json::to_string(&revision.proposed)?,
                revision.made_at_ms,
                (!revision.images.is_empty()).then(|| serde_json::to_string(&revision.images)).transpose()?,
                (!revision.changed.is_empty()).then_some(revision.changed.as_str()),
                key.as_str(),
            ],
        )?;
        let made = proposal_on(&tx, id)?;
        tx.commit()?;
        Ok(Revising::Wrote(Made::New(made)))
    }

    /// Withdraws the proposal `id`, as `withdrawal` says, unless it's withdrawn already, or the person has chosen in
    /// its newest revision, which leave it as it was. Checked in the commit that withdraws it, so an answer given
    /// meanwhile is never withdrawn under.
    pub fn withdraw_proposal(&self, id: &ProposalId, withdrawal: &Withdrawal) -> Result<Withdrawing, StoreError> {
        let mut conn = lock(&self.writer);
        let tx = conn.transaction()?;
        let earlier = proposal_on(&tx, id)?;
        match earlier.standing() {
            RevisionStanding::Waiting | RevisionStanding::AnotherRound(_) => {}
            RevisionStanding::Withdrawn(_) => return Ok(Withdrawing::Already(earlier)),
            RevisionStanding::Chose(_) | RevisionStanding::Responded => return Ok(Withdrawing::Settled(earlier)),
        }
        tx.execute(
            "UPDATE proposals SET withdrawn = ? WHERE id = ?",
            params![serde_json::to_string(withdrawal)?, id.as_str()],
        )?;
        let withdrawn = proposal_on(&tx, id)?;
        tx.commit()?;
        Ok(Withdrawing::Withdrew(withdrawn))
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
                "SELECT {} FROM proposal_revisions r
                     WHERE proposal_id = ? AND number >= ? ORDER BY number LIMIT ?",
                RevisionColumns::LIST
            ))?
            .query_map(params![id.as_str(), from.0, limit], |row| revision_from_row(row, RevisionColumns::at(0)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        tx.commit()?;
        Ok((revisions, newest))
    }
}
