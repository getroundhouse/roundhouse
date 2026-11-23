//! What's held of GitHub: what each view shows, and what the hub sent of every workspace's own pull request and of the
//! issues and pull requests views show as `GithubUpdate`, as a hub from before observations sends them. What a hub
//! sends as observations is held with every other observation, in the copy of the hub's records, which views read here
//! through what [`Observed`] gives. A hub sends each thing in one form, so taking in one form of something lets go of
//! the other, which only a connection to another hub can have left, and what a view reads comes from whichever is held.

use std::collections::{BTreeMap, HashMap};

use protocol::client::github::{GithubUpdate, MOST_REFERENCES_SHOWN};
use protocol::client::{HubNotice, ObservationNotice};
use protocol::model::{
    Level, Observation, ObservationKey, Observed as Freshness, OwnPullRequestSaid, PullRequestFound, ReferenceAsk,
    ReferenceRead, RepositoryUnseen, WorkspaceId,
};

use super::as_read;
use super::views::ShownOfGithub;

/// The observation a key names, where the copy of the hub's records holds it.
pub(super) type Observed<'a> = &'a dyn Fn(&ObservationKey) -> Option<&'a Observation>;

/// What each view shows, and what the hub has sent of what any shows as `GithubUpdate`.
#[derive(Default)]
pub(super) struct GithubHeld {
    /// By ID, which is in the order they were made, so all they show is said in the same order each time.
    views: BTreeMap<u64, View>,
    references: HashMap<ReferenceAsk, ReferenceRead>,
    /// Of every workspace, which the hub's level sends.
    pull_requests: HashMap<WorkspaceId, Own>,
    /// Whether the hub's level has sent a workspace's pull request as an observation since the connection began, so a
    /// workspace it sent nothing of isn't known, rather than not yet said.
    hub_observes: bool,
}

/// What the hub said of a workspace's own pull request: what was found, `None` where it said it hasn't looked, and
/// whether GitHub last showed the default its repository at all.
#[derive(Clone, PartialEq)]
struct Own {
    found: Option<PullRequestFound>,
    unseen: Option<RepositoryUnseen>,
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
            | ObservationKey::GithubUsername { .. }
            | ObservationKey::ModelCatalog { .. }
            | ObservationKey::AccountIdentity { .. }
            | ObservationKey::AccountLimits { .. }
            | ObservationKey::SessionContext { .. }
            | ObservationKey::SessionTodos { .. }
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

    /// Lets go of what the hub sent of the issues and pull requests no view shows now, since the hub sends it again once
    /// one does. Every workspace's own pull request is kept, since the hub sends what changes of each.
    pub(super) fn forget_unshown(&mut self) {
        let shown = self.shown();
        self.references.retain(|ask, _| shown.references.contains(ask));
    }

    /// Takes in `update`, telling each view that shows what it's of. Returns whether it changed anything. What the
    /// copy of the hub's records holds of the same, as an observation, the caller lets go of.
    pub(super) fn apply(&mut self, update: GithubUpdate) -> bool {
        let shows: Box<dyn Fn(&ShownOfGithub) -> bool> = match update {
            GithubUpdate::Reference { reference, read } => {
                if self.references.get(&reference) == Some(&read) {
                    return false;
                }
                self.references.insert(reference.clone(), read);
                Box::new(move |shown| shown.references.contains(&reference))
            }
            GithubUpdate::PullRequest { workspace, found, unseen } => {
                self.hub_observes = false;
                let own = Own { found, unseen };
                if self.pull_requests.get(&workspace) == Some(&own) {
                    return false;
                }
                self.pull_requests.insert(workspace.clone(), own);
                Box::new(move |shown| shown.workspaces.contains(&workspace))
            }
            GithubUpdate::Unknown => return false,
        };
        for view in self.views.values().filter(|view| shows(&view.shown)) {
            let _ = view.changed.try_send(());
        }
        true
    }

    /// Takes in that the copy of the hub's records has taken in observations of GitHub that `keys` name, which
    /// `changed` says changed what it holds: lets go of what `GithubUpdate` brought of each, and tells each view that
    /// shows one that changed.
    pub(super) fn observed(&mut self, keys: &[ObservationKey], changed: bool) {
        for key in keys.iter().filter(|key| key.is_github()) {
            if key.level() == Some(Level::Hub) {
                self.hub_observes = true;
            }
            match key {
                ObservationKey::WorkspacePullRequest { workspace_id }
                | ObservationKey::WorkspacePullRequestDetail { workspace_id }
                | ObservationKey::WorkspaceRepositoryUnseen { workspace_id } => {
                    self.pull_requests.remove(workspace_id);
                }
                ObservationKey::Reference { reference } => {
                    self.references.remove(reference);
                }
                ObservationKey::WorkspaceLines { .. }
                | ObservationKey::WorkspaceChanges { .. }
                | ObservationKey::GithubUsername { .. }
                | ObservationKey::ModelCatalog { .. }
                | ObservationKey::AccountIdentity { .. }
                | ObservationKey::AccountLimits { .. }
                | ObservationKey::SessionContext { .. }
                | ObservationKey::SessionTodos { .. }
                | ObservationKey::SessionCache { .. }
                | ObservationKey::WaitRead { .. }
                | ObservationKey::Unknown => {}
            }
        }
        if !changed {
            return;
        }
        for view in self.views.values().filter(|view| keys.iter().any(|key| view.shows(key))) {
            let _ = view.changed.try_send(());
        }
    }

    /// Takes in that a new connection's hub has said where its level stands, which says whether it sends a workspace's
    /// pull request as observations, as `keys`, the observations of GitHub in its list, say.
    pub(super) fn relisted(&mut self, keys: &[ObservationKey], changed: bool) {
        self.hub_observes = false;
        self.observed(keys, changed);
    }

    pub(super) fn reference(&self, ask: &ReferenceAsk, observed: Observed<'_>) -> Option<ReferenceRead> {
        let key = ObservationKey::Reference { reference: ask.clone() };
        if let Some(Observation::Reference { said, .. }) = observed(&key) {
            let (said, read_at_ms) = fresh(said)?;
            return as_read::reference(said, read_at_ms);
        }
        self.references.get(ask).cloned()
    }

    /// `workspace`'s pull request: how it stands, as the hub's level sent it, with the checks, history and stack the
    /// workspace's level sent where they're of it at the head it's at, and none otherwise, which the bar draws as
    /// reading. Or as `GithubUpdate` brought it, where the hub sent it so.
    pub(super) fn pull_request(&self, workspace: &WorkspaceId, observed: Observed<'_>) -> Option<PullRequestFound> {
        let key = ObservationKey::WorkspacePullRequest { workspace_id: workspace.clone() };
        let Some(Observation::WorkspacePullRequest { pull_request, .. }) = observed(&key) else {
            return self.pull_requests.get(workspace).and_then(|own| own.found.clone());
        };
        let (OwnPullRequestSaid { pull_request: own }, read_at_ms) = fresh(pull_request)?;
        let detail_key = ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace.clone() };
        let detail = match observed(&detail_key) {
            Some(Observation::WorkspacePullRequestDetail { detail, .. }) => {
                fresh(detail).and_then(|(detail, _)| detail.pull_request.as_ref())
            }
            _ => None,
        };
        Some(PullRequestFound {
            pull_request: own.as_ref().map(|own| as_read::own_pull_request(own, detail, read_at_ms)),
        })
    }

    pub(super) fn unseen(&self, workspace: &WorkspaceId, observed: Observed<'_>) -> Option<RepositoryUnseen> {
        let key = ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace.clone() };
        if let Some(Observation::WorkspaceRepositoryUnseen { unseen, .. }) = observed(&key) {
            return fresh(unseen).and_then(|(unseen, _)| unseen.clone());
        }
        self.pull_requests.get(workspace).and_then(|own| own.unseen.clone())
    }

    /// Whether the hub has said what it knows of `workspace`'s pull request, even that it hasn't looked: in either
    /// form, or by sending the hub's level's observations on this connection, where one not sent isn't known.
    pub(super) fn said_pull_request(&self, workspace: &WorkspaceId, observed: Observed<'_>) -> bool {
        let key = ObservationKey::WorkspacePullRequest { workspace_id: workspace.clone() };
        self.hub_observes || observed(&key).is_some() || self.pull_requests.contains_key(workspace)
    }

    /// Whether `GithubUpdate` brought what `key` is of, for a test of which form each is held in.
    #[cfg(test)]
    pub(super) fn held_as_update(&self, key: &ObservationKey) -> bool {
        match key {
            ObservationKey::WorkspacePullRequest { workspace_id }
            | ObservationKey::WorkspacePullRequestDetail { workspace_id }
            | ObservationKey::WorkspaceRepositoryUnseen { workspace_id } => {
                self.pull_requests.contains_key(workspace_id)
            }
            ObservationKey::Reference { reference } => self.references.contains_key(reference),
            ObservationKey::WorkspaceLines { .. }
            | ObservationKey::WorkspaceChanges { .. }
            | ObservationKey::GithubUsername { .. }
            | ObservationKey::ModelCatalog { .. }
            | ObservationKey::AccountIdentity { .. }
            | ObservationKey::AccountLimits { .. }
            | ObservationKey::SessionContext { .. }
            | ObservationKey::SessionTodos { .. }
            | ObservationKey::SessionCache { .. }
            | ObservationKey::WaitRead { .. }
            | ObservationKey::Unknown => false,
        }
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

/// Which observations a workspace's own pull request is sent as.
pub(in crate::client) fn own_keys(workspace: &WorkspaceId) -> [ObservationKey; 3] {
    [
        ObservationKey::WorkspacePullRequest { workspace_id: workspace.clone() },
        ObservationKey::WorkspacePullRequestDetail { workspace_id: workspace.clone() },
        ObservationKey::WorkspaceRepositoryUnseen { workspace_id: workspace.clone() },
    ]
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
