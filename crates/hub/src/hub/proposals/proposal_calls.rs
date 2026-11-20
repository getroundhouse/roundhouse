use super::super::{Hub, new_id};
use crate::store::{Made, NewRevision, Revising, Withdrawing, now_ms};
use protocol::client::proposals::{ProposalRevisions, ProposalRevisionsRead};
use protocol::client::{ApiError, ErrorCode, HubNotice, records};
use protocol::model::{Proposal, ProposalId, ProposalRevision, RevisionNumber, SessionId, Withdrawal, WorkspaceId};
use protocol::worker::{ProposalRecord, Propose, ReadProposal, Revise, Withdraw};

impl Hub {
    /// Makes the proposal the agent in `session_id` asked for, in its workspace, `workspace_id`, once it's been
    /// checked, with the images it points to, and tells the feed. Asked again with the same key, it answers with the
    /// one it made.
    pub(crate) async fn propose(
        &self,
        workspace_id: &WorkspaceId,
        session_id: SessionId,
        asked: Propose,
    ) -> Result<ProposalRecord, ApiError> {
        let Propose { proposed, call, key, images, attachments } = asked;
        proposed.check()?;
        let (asking, asked_key) = (session_id.clone(), key.clone());
        // Before its images are kept, so asking again for one already made keeps nothing more.
        if let Some(proposal) = self.blocking(move |store| store.proposal_asked(&asking, &asked_key)).await? {
            return Ok(ProposalRecord { proposal, revision: None });
        }
        // Before it's made, so a client never asks for one before the hub has it, as for a reply's.
        self.keep_from_agent(&session_id, attachments).await;
        let made_at_ms = now_ms();
        let current = ProposalRevision {
            number: RevisionNumber::FIRST,
            changed: String::new(),
            call,
            proposed,
            made_at_ms,
            response: None,
            images,
        };
        let proposal = Proposal {
            id: new_id().into(),
            workspace_id: workspace_id.clone(),
            session_id,
            revisions: vec![current.summary()],
            withdrawn: None,
            current,
            created_at_ms: made_at_ms,
            rev: protocol::model::Rev::default(),
        };
        let proposal = match self.blocking(move |store| store.insert_proposal(&proposal, &key)).await? {
            Made::New(made) => {
                self.notify(HubNotice::Proposal(made.clone()));
                made
            }
            Made::Earlier(made) => made,
        };
        Ok(ProposalRecord { proposal, revision: None })
    }

    /// Writes a new revision of one of the proposals of the chat `session_id`, once it's been checked, with the images
    /// it points to, and tells the feed, whether or not the person has answered the newest. Another chat's is refused
    /// as not found, and one the agent withdrew as a conflict. Asked again with the same key, it answers with the
    /// proposal as it is now.
    pub(crate) async fn revise(&self, session_id: &SessionId, asked: Revise) -> Result<ProposalRecord, ApiError> {
        let Revise { id, proposed, changed, call, key, images, attachments } = asked;
        proposed.check()?;
        if changed.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "say what changed since the revision before"));
        }
        self.read_proposal(session_id, id.clone()).await?;
        // Before it's written, so a client never asks for one before the hub has it, as for a reply's.
        self.keep_from_agent(session_id, attachments).await;
        let revision =
            NewRevision { changed: changed.trim().to_string(), call, proposed, made_at_ms: now_ms(), images };
        let proposal = match self.blocking(move |store| store.revise_proposal(&id, &revision, &key)).await? {
            Revising::Wrote(Made::New(made)) => {
                self.notify(HubNotice::Proposal(made.clone()));
                made
            }
            Revising::Wrote(Made::Earlier(made)) => made,
            Revising::Withdrawn(_) => {
                let withdrawn = "you withdrew that proposal, so it can't be revised: propose it again instead";
                return Err(ApiError::new(ErrorCode::Conflict, withdrawn));
            }
        };
        Ok(ProposalRecord { proposal, revision: None })
    }

    /// Withdraws one of the proposals of the chat `session_id`, so it no longer waits for the person, and tells the
    /// feed. Another chat's is refused as not found, one without why as an invalid request, and one the person has
    /// chosen in as a conflict. Asked again, it answers with the proposal as it was withdrawn.
    pub(crate) async fn withdraw(&self, session_id: &SessionId, asked: Withdraw) -> Result<ProposalRecord, ApiError> {
        let Withdraw { id, why, call } = asked;
        if why.trim().is_empty() {
            return Err(ApiError::new(ErrorCode::InvalidRequest, "say why you're withdrawing it"));
        }
        self.read_proposal(session_id, id.clone()).await?;
        let withdrawal = Withdrawal { why: why.trim().to_string(), call, at_ms: now_ms() };
        let proposal = match self.blocking(move |store| store.withdraw_proposal(&id, &withdrawal)).await? {
            Withdrawing::Withdrew(withdrawn) => {
                self.notify(HubNotice::Proposal(withdrawn.clone()));
                withdrawn
            }
            Withdrawing::Already(withdrawn) => withdrawn,
            Withdrawing::Settled(_) => {
                let settled = "the person has already answered that proposal, so there's nothing to withdraw";
                return Err(ApiError::new(ErrorCode::Conflict, settled));
            }
        };
        Ok(ProposalRecord { proposal, revision: None })
    }

    /// One of the proposals of the chat `session_id`. Another chat's is refused as not found, so an agent reads only
    /// what was proposed in its own chat.
    pub(crate) async fn read_proposal(&self, session_id: &SessionId, id: ProposalId) -> Result<Proposal, ApiError> {
        let proposal = self.blocking(move |store| store.get_proposal(&id)).await?;
        if proposal.session_id != *session_id {
            return Err(ApiError::new(ErrorCode::NotFound, "that proposal isn't in this chat"));
        }
        Ok(proposal)
    }

    /// One of the proposals of the chat `session_id`, as [`ReadProposal`] says, with its revision `revision` in full
    /// when that isn't the newest.
    pub(crate) async fn read_proposal_revision(
        &self,
        session_id: &SessionId,
        ReadProposal { id, revision }: ReadProposal,
    ) -> Result<ProposalRecord, ApiError> {
        let proposal = self.read_proposal(session_id, id.clone()).await?;
        let Some(number) = revision.filter(|number| *number != proposal.current.number) else {
            return Ok(ProposalRecord { proposal, revision: None });
        };
        let (mut read, _) = self.blocking(move |store| store.proposal_revisions(&id, number, 1)).await?;
        let revision = read
            .pop()
            .filter(|read| read.number == number)
            .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("the proposal has no revision {number}")))?;
        Ok(ProposalRecord { proposal, revision: Some(Box::new(revision)) })
    }

    /// Some of the revisions of the proposal `read.id`, as [`ProposalRevisionsRead`] says.
    pub(crate) async fn proposal_revisions(&self, read: ProposalRevisionsRead) -> Result<ProposalRevisions, ApiError> {
        let ProposalRevisionsRead { id, from, limit } = read;
        let limit = limit.clamp(1, ProposalRevisionsRead::MOST);
        let (revisions, newest) = self.blocking(move |store| store.proposal_revisions(&id, from, limit)).await?;
        Ok(ProposalRevisions { revisions, newest })
    }

    /// The session's proposals, oldest first, each with its newest revision, with the revision they're as of.
    pub(crate) async fn session_proposals(&self, id: SessionId) -> Result<records::Proposals, ApiError> {
        self.blocking(move |store| {
            store.get_session(&id)?;
            let (proposals, rev) = store.list_proposals(&id)?;
            Ok(records::Proposals { proposals, rev })
        })
        .await
    }
}
