//! Replies that carry a record, taken into the copy of the hub's records as they come, so what's shown needn't wait for
//! the feed to say the same, and nobody who makes a call has to hand on what it answered.

use parking_lot::Mutex;
use protocol::client::ClientResponse;
use protocol::model::Record;

use super::super::follow::{Follow, Reach};
use crate::records::HubRecords;

/// The copies of the hub's records that a connection's replies go into: those of each [`RecordsFeed`](super::RecordsFeed)
/// that has followed it, for as long as it's kept.
#[derive(Default)]
pub(in super::super) struct Folding {
    into: Mutex<Vec<Reach<HubRecords>>>,
}

impl Folding {
    /// Takes every reply from here on into what `records` follows too, unless it already does.
    pub(super) fn reach(&self, records: &Follow<HubRecords>) {
        let mut into = self.into.lock();
        if !into.iter().any(|reach| reach.reaches(records)) {
            into.push(records.reach());
        }
    }

    /// Takes each record `response` carries into every copy, letting go of those no longer kept. A copy keeps whichever
    /// of the reply and what the feed tells of is newer, so it doesn't matter which comes first.
    pub(in super::super) fn fold(&self, response: &ClientResponse) {
        let carried = carried(response);
        if carried.is_empty() {
            return;
        }
        self.into.lock().retain(|reach| {
            reach.change(|records| {
                let mut changed = false;
                for record in &carried {
                    changed |= records.apply_record(record.clone());
                }
                changed
            })
        });
    }
}

/// The records `response` carries. A list carries none, since it's of every record of a kind, or of a session, and a
/// record missing from it is gone, which only the feed's own list can say, as only it is read at a revision. And an
/// older hub's lists have no revisions, so one read on another thread could put an older copy in place of a newer.
fn carried(response: &ClientResponse) -> Vec<Record> {
    match response {
        ClientResponse::HubSettings(settings) => vec![Record::HubSettings(settings.clone())],
        ClientResponse::NewWorkspaceDraft(draft) => vec![Record::NewWorkspaceDraft(draft.clone())],
        ClientResponse::NewWorkspaceMade(made) => {
            vec![Record::Workspace(made.workspace.clone()), Record::Session(made.session.clone())]
        }
        ClientResponse::Repo(repo) => vec![Record::Repo(repo.clone())],
        ClientResponse::ModelList(list) => vec![Record::ModelList(list.clone())],
        ClientResponse::Workspace(workspace) => vec![Record::Workspace(workspace.clone())],
        ClientResponse::Comment(comment) => vec![Record::Comment(comment.clone())],
        ClientResponse::Session(session) => vec![Record::Session(session.clone())],
        ClientResponse::Account(account) => vec![Record::Account(account.clone())],
        ClientResponse::GithubAccount(account) => vec![Record::GithubAccount(account.clone())],
        ClientResponse::GithubSignIn(sign_in) => vec![Record::GithubSignIn(sign_in.clone())],
        ClientResponse::GithubApp(app) => vec![Record::GithubApp(app.clone())],
        ClientResponse::Terminal(terminal) => vec![Record::Terminal(terminal.clone())],
        ClientResponse::Proposal(proposal) => vec![Record::Proposal(proposal.clone())],
        ClientResponse::Repos(_)
        | ClientResponse::Records(_)
        | ClientResponse::Workspaces(_)
        | ClientResponse::Comments(_)
        | ClientResponse::Sessions(_)
        | ClientResponse::Subagents(_)
        | ClientResponse::Proposals(_)
        // And what isn't a record.
        | ClientResponse::Paired(_)
        | ClientResponse::Devices(_)
        | ClientResponse::Pairing(_)
        | ClientResponse::Device(_)
        | ClientResponse::Hello(_)
        | ClientResponse::Ok
        | ClientResponse::HubStatus(_)
        | ClientResponse::Agents(_)
        | ClientResponse::GitStatus(_)
        | ClientResponse::GitDiff(_)
        | ClientResponse::SetupLog(_)
        | ClientResponse::PullRequest(_)
        | ClientResponse::References(_)
        | ClientResponse::Events(_)
        | ClientResponse::PlacedEvents(_)
        | ClientResponse::Prompts(_)
        | ClientResponse::Compactions(_)
        | ClientResponse::Found(_)
        | ClientResponse::PromptRecorded(_)
        | ClientResponse::Attachment(_)
        | ClientResponse::AttachmentData(_)
        | ClientResponse::SessionAccount(_)
        | ClientResponse::GithubAppManifest(_)
        | ClientResponse::WorktreeFiles(_)
        | ClientResponse::FileContents(_)
        | ClientResponse::FileOpened(_)
        | ClientResponse::FilePages(_)
        // A proposal's revisions, but for its newest, which only the proposal carries.
        | ClientResponse::ProposalRevisions(_) => Vec::new(),
    }
}
