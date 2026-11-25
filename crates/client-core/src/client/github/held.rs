//! What's held of GitHub: what each view shows. What the hub sends of it, as observations, is held with every other
//! observation, in the copy of the hub's records, which views read here through what [`HeldObservations`] gives.

use std::collections::BTreeMap;

use protocol::client::github::MOST_REFERENCES_SHOWN;
use protocol::client::{HubNotice, ObservationNotice};
use protocol::model::{
    Observation, ObservationKey, Observed as Freshness, OwnPullRequestSaid, PullRequestFound, ReferenceAsk,
    ReferenceRead, RepositoryUnseen, WorkspaceId,
};

use super::as_read;
use super::views::ShownOfGithub;

/// The observation a key names, where the copy of the hub's records holds it, and whether the hub's level has listed
/// every observation it sends there, so one it lacks isn't known.
pub(super) struct HeldObservations<'a> {
    pub(super) of: &'a dyn Fn(&ObservationKey) -> Option<&'a Observation>,
    pub(super) hub_listed: bool,
}

/// What each view shows.
#[derive(Default)]
pub(super) struct GithubHeld {
    /// By ID, which is in the order they were made, so all they show is said in the same order each time.
    views: BTreeMap<u64, View>,
}

/// What one view shows, and what tells it that some of that changed.
struct View {
    shown: ShownOfGithub,
    changed: flume::Sender<()>,
}

impl View {
    /// Whether it shows what `key` is of.
    fn shows(&self, key: &ObservationKey) -> bool {
        match key {
            ObservationKey::WorkspacePullRequest { workspace_id }
            | ObservationKey::WorkspacePullRequestDetail { workspace_id }
            | ObservationKey::WorkspaceRepositoryUnseen { workspace_id } => {
                self.shown.workspaces.contains(workspace_id)
            }
            ObservationKey::Reference { reference } => self.shown.references.contains(reference),
            ObservationKey::WorkspaceLines { .. }
            | ObservationKey::WorkspaceChanges { .. }
            | ObservationKey::WorkspaceBranches { .. }
            | ObservationKey::WorkspaceWorkerBuild { .. }
            | ObservationKey::ReleaseOffered
            | ObservationKey::RepositoryReach { .. }
            | ObservationKey::GithubUsername { .. }
            | ObservationKey::GithubAppSaid { .. }
            | ObservationKey::GithubAppBot { .. }
            | ObservationKey::InstalledOn { .. }
            | ObservationKey::ModelCatalog { .. }
            | ObservationKey::AccountIdentity { .. }
            | ObservationKey::AccountLimits { .. }
            | ObservationKey::SessionContext { .. }
            | ObservationKey::SessionTodos { .. }
            | ObservationKey::SessionToolServers { .. }
            | ObservationKey::SessionCache { .. }
            | ObservationKey::WaitRead { .. }
            | ObservationKey::Unknown => false,
        }
    }
}

impl GithubHeld {
    /// Adds the view `view`, which shows nothing yet, and is told of a change on `changed`.
    pub(super) fn add(&mut self, view: u64, changed: flume::Sender<()>) {
        self.views.insert(view, View { shown: ShownOfGithub::default(), changed });
    }

    /// Takes `shown` to be what the view `view` shows.
    pub(super) fn show(&mut self, view: u64, shown: ShownOfGithub) {
        if let Some(view) = self.views.get_mut(&view) {
            view.shown = shown;
        }
    }

    pub(super) fn hide(&mut self, view: u64) {
        self.views.remove(&view);
    }

    /// All that every view shows, each once, but for chips past the most a hub takes, which are those of the views made
    /// longest ago, since the newest is likeliest to be what's in front.
    pub(super) fn shown(&self) -> ShownOfGithub {
        let mut all = ShownOfGithub::default();
        for View { shown, .. } in self.views.values().rev() {
            add(&mut all.workspaces, &shown.workspaces);
            add(&mut all.references, &shown.references);
        }
        all.references.truncate(MOST_REFERENCES_SHOWN);
        all
    }

    /// Takes in that the copy of the hub's records has taken in observations of GitHub that `keys` name, which
    /// `changed` says changed what it holds, telling each view that shows one.
    pub(super) fn observed(&self, keys: &[ObservationKey], changed: bool) {
        if !changed {
            return;
        }
        for view in self.views.values().filter(|view| keys.iter().any(|key| view.shows(key))) {
            let _ = view.changed.try_send(());
        }
    }

    /// What the hub said of `ask`, as a chip's level sent it. `None` until it has.
    pub(super) fn reference(ask: &ReferenceAsk, observed: &HeldObservations<'_>) -> Option<ReferenceRead> {
        let key = ObservationKey::Reference { reference: ask.clone() };
        let Some(Observation::Reference { said, .. }) = (observed.of)(&key) else { return None };
        let (said, read_at_ms) = fresh(said)?;
        as_read::reference(said, read_at_ms)
    }

    /// `workspace`'s pull request: how it stands, as the hub's level sent it, with the checks, history and stack the
    /// workspace's level sent where they're of it at the head it's at, and none otherwise, which the bar draws as
    /// reading.
    pub(super) fn pull_request(workspace: &WorkspaceId, observed: &HeldObservations<'_>) -> Option<PullRequestFound> {
        let key = ObservationKey::WorkspacePullRequest { workspace_id: workspace.clone() };
        let Some(Observation::WorkspacePullRequest { pull_request, .. }) = (observed.of)(&key) else { return None };
        let (OwnPullRequestSaid { pull_request: own }, read_at_ms) = fresh(pull_request)?;
        let detail_key = ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace.clone() };
        let held = (observed.of)(&detail_key);
        let detail = if let Some(Observation::WorkspacePullRequestDetail { detail, .. }) = held {
            fresh(detail).and_then(|(detail, _)| detail.pull_request.as_ref())
        } else {
            None
        };
        Some(PullRequestFound {
            pull_request: own.as_ref().map(|own| as_read::own_pull_request(own, detail, read_at_ms)),
        })
    }

    /// The repository GitHub last showed the default none of, where `workspace`'s branch's pull requests are, as the
    /// workspace's level sent it.
    pub(super) fn unseen(workspace: &WorkspaceId, observed: &HeldObservations<'_>) -> Option<RepositoryUnseen> {
        let key = ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace.clone() };
        let Some(Observation::WorkspaceRepositoryUnseen { unseen, .. }) = (observed.of)(&key) else { return None };
        fresh(unseen).and_then(|(unseen, _)| unseen.clone())
    }

    /// Whether the hub has said what it knows of `workspace`'s pull request, even that it hasn't looked: by sending its
    /// observation, or by listing the hub's level, after which one it lacks isn't known.
    pub(super) fn said_pull_request(workspace: &WorkspaceId, observed: &HeldObservations<'_>) -> bool {
        let key = ObservationKey::WorkspacePullRequest { workspace_id: workspace.clone() };
        observed.hub_listed || (observed.of)(&key).is_some()
    }
}

/// The keys of the observations of GitHub among `observations`.
pub(in crate::client) fn github_keys(observations: &[Observation]) -> Vec<ObservationKey> {
    observations.iter().map(Observation::key).filter(ObservationKey::is_github).collect()
}

/// The key of the observation of GitHub that `notice` tells of, if it tells of one.
pub(in crate::client) fn notice_key(notice: &HubNotice) -> Option<ObservationKey> {
    let key = match ObservationNotice::try_from(notice.clone()).ok()? {
        ObservationNotice::Copy(copy) => copy.key(),
        ObservationNotice::Removed { observation, .. } => observation,
    };
    key.is_github().then_some(key)
}

/// What `observed` says, and as of when, as the form a read of GitHub has keeps it. One that's current has no time of
/// its own, so it's as of 0, which reads the same however often it's read, and no view reads. `None` for one not known.
fn fresh<T>(observed: &Freshness<T>) -> Option<(&T, i64)> {
    match observed {
        Freshness::Current { value } => Some((value, 0)),
        Freshness::AsOf { value, as_of_ms } => Some((value, *as_of_ms)),
        Freshness::NotKnown | Freshness::Unknown => None,
    }
}

/// Adds to `all` each of `more` it doesn't have.
fn add<T: PartialEq + Clone>(all: &mut Vec<T>, more: &[T]) {
    for one in more {
        if !all.contains(one) {
            all.push(one.clone());
        }
    }
}
