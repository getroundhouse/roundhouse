use super::super::{Hub, new_id};
use crate::store::{Made, now_ms};
use protocol::client::proposals::{ProposalRevisions, ProposalRevisionsRead};
use protocol::client::{ApiError, ErrorCode, HubNotice, records};
use protocol::model::{Proposal, ProposalId, ProposalRevision, RevisionNumber, SessionId, WorkspaceId};
use protocol::worker::{ProposalRecord, Propose};

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
        if let Some(proposal) = self.blocking(move |store| store.proposal_asked(&asking, &asked_key)).await? {
            return Ok(ProposalRecord { proposal });
        }
        // Before it's made, so a client never asks for one before the hub has it, as for a reply's.
        self.keep_from_agent(&session_id, attachments).await;
        let made_at_ms = now_ms();
        let current =
            ProposalRevision { number: RevisionNumber::FIRST, call, proposed, made_at_ms, response: None, images };
        let proposal = Proposal {
            id: new_id().into(),
            workspace_id: workspace_id.clone(),
            session_id,
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
        Ok(ProposalRecord { proposal })
    }

    /// One of the proposals of the chat `session_id`. Another chat's is refused as not found, so an agent reads only
    /// what was proposed in its own chat.
    pub(crate) async fn read_proposal(
        &self,
        session_id: &SessionId,
        id: ProposalId,
    ) -> Result<ProposalRecord, ApiError> {
        let proposal = self.blocking(move |store| store.get_proposal(&id)).await?;
        if proposal.session_id != *session_id {
            return Err(ApiError::new(ErrorCode::NotFound, "that proposal isn't in this chat"));
        }
        Ok(ProposalRecord { proposal })
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
