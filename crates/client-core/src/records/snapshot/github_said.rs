//! What GitHub says of each workspace's own pull request and of the issues and pull requests chips name, as views read
//! it: in the form the hub's observations hold it, each as fresh as the hub said.

use protocol::model::{
    Observation, ObservationKey, Observed, OwnSummary, PullRequestDetail, PullRequestSaid, ReferenceAsk, ReferenceSaid,
    RepositoryUnseen, WorkspaceId,
};

use super::observed::read;
use super::{Changes, Snapshot};

/// A workspace's own pull request: what its summary says and how the worktree stands against it, as the hub's level
/// sent them, and its checks, history and stack, as the workspace's own level sent them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspacePullRequest<'a> {
    pub own: &'a OwnSummary,
    /// Not known but while the workspace is followed at its own level, and where what was sent is of another pull
    /// request, or of it at another head, so a push is never shown with the checks of the commit before it.
    pub detail: Observed<&'a PullRequestDetail>,
}

/// What a view reads of a repository GitHub showed none of that the client doesn't hold.
const UNSEEN_NOT_KNOWN: Observed<Option<RepositoryUnseen>> = Observed::NotKnown;

/// The same, of an issue or a pull request a chip names.
const REFERENCE_NOT_KNOWN: Observed<ReferenceSaid> = Observed::NotKnown;

impl Snapshot {
    /// `workspace`'s own pull request, as the hub's level sent it, or `None` inside where its branch has none. Not known
    /// until the hub has sent it, and from a hub whose cache has forgotten it.
    pub fn workspace_pull_request(&self, workspace: &WorkspaceId) -> Observed<Option<WorkspacePullRequest<'_>>> {
        let key = ObservationKey::WorkspacePullRequest { workspace_id: workspace.clone() };
        let Some(Observation::WorkspacePullRequest { pull_request, .. }) = self.observation(&key) else {
            return Observed::NotKnown;
        };
        read(pull_request, |said| {
            let own = said.pull_request.as_ref()?;
            Some(WorkspacePullRequest { own, detail: self.workspace_pull_request_detail(workspace, &own.pull_request) })
        })
    }

    /// The checks, history and stack of `workspace`'s own pull request, as its own level sent them, where they're of
    /// `said`, at the head it's at.
    fn workspace_pull_request_detail(
        &self,
        workspace: &WorkspaceId,
        said: &PullRequestSaid,
    ) -> Observed<&PullRequestDetail> {
        let key = ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace.clone() };
        let Some(Observation::WorkspacePullRequestDetail { detail, .. }) = self.observation(&key) else {
            return Observed::NotKnown;
        };
        let of_it = |detail: &&PullRequestDetail| {
            detail.repository == said.repository && detail.number == said.number && detail.head == said.summary.head
        };
        match read(detail, |detail| detail.pull_request.as_ref().filter(of_it)) {
            Observed::Current { value: Some(detail) } => Observed::Current { value: detail },
            Observed::AsOf { value: Some(detail), as_of_ms } => Observed::AsOf { value: detail, as_of_ms },
            Observed::Current { value: None } | Observed::AsOf { value: None, .. } | Observed::NotKnown => {
                Observed::NotKnown
            }
            Observed::Unknown => Observed::Unknown,
        }
    }

    /// The repository GitHub last showed the default GitHub account none of, where `workspace`'s branch's pull requests
    /// are, as the workspace's own level sent it, or `None` inside where it showed it. While it shows none, what
    /// [`Snapshot::workspace_pull_request`] says is only what was known before. Not known but while the workspace is
    /// followed at its own level.
    pub fn workspace_repository_unseen(&self, workspace: &WorkspaceId) -> &Observed<Option<RepositoryUnseen>> {
        let key = ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace.clone() };
        let Some(Observation::WorkspaceRepositoryUnseen { unseen, .. }) = self.observation(&key) else {
            return &UNSEEN_NOT_KNOWN;
        };
        unseen
    }

    /// What GitHub says of the issue or pull request `ask` names, as its own level sent it, which it does while a view
    /// shows it as a chip. Not known otherwise, and until it's sent.
    pub fn reference_said(&self, ask: &ReferenceAsk) -> &Observed<ReferenceSaid> {
        let key = ObservationKey::Reference { reference: ask.clone() };
        let Some(Observation::Reference { said, .. }) = self.observation(&key) else { return &REFERENCE_NOT_KNOWN };
        said
    }
}

impl Changes<'_> {
    /// Whether what GitHub says of the issue or pull request `ask` names changed, came or went.
    pub fn reference_said(&self, ask: &ReferenceAsk) -> bool {
        self.observation(&ObservationKey::Reference { reference: ask.clone() })
    }
}
