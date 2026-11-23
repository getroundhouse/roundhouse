//! What's held of GitHub: what each view shows, what the hub has sent of the issues and pull requests they show, and
//! what it sends of every workspace's own pull request.

use std::collections::{BTreeMap, HashMap};

use protocol::client::github::{GithubShown, GithubUpdate, MOST_REFERENCES_SHOWN};
use protocol::model::{PullRequestFound, ReferenceAsk, ReferenceRead, RepositoryUnseen, WorkspaceId};

/// What each view shows, and what the hub has sent of what any shows.
#[derive(Default)]
pub(super) struct GithubHeld {
    /// By ID, which is in the order they were made, so all they show is said in the same order each time.
    views: BTreeMap<u64, Shown>,
    references: HashMap<ReferenceAsk, ReferenceRead>,
    /// Of every workspace, which the hub's level sends.
    pull_requests: HashMap<WorkspaceId, Own>,
}

/// What the hub said of a workspace's own pull request: what was found, `None` where it said it hasn't looked, and
/// whether GitHub last showed the default its repository at all.
#[derive(Clone, PartialEq)]
struct Own {
    found: Option<PullRequestFound>,
    unseen: Option<RepositoryUnseen>,
}

/// What one view shows, and what tells it that some of that changed.
struct Shown {
    shown: GithubShown,
    changed: flume::Sender<()>,
}

impl GithubHeld {
    /// Adds the view `view`, which shows nothing yet, and is told of a change on `changed`.
    pub(super) fn add(&mut self, view: u64, changed: flume::Sender<()>) {
        self.views.insert(view, Shown { shown: GithubShown::default(), changed });
    }

    /// Takes `shown` to be what the view `view` shows.
    pub(super) fn show(&mut self, view: u64, shown: GithubShown) {
        if let Some(view) = self.views.get_mut(&view) {
            view.shown = shown;
        }
    }

    pub(super) fn hide(&mut self, view: u64) {
        self.views.remove(&view);
    }

    /// All that every view shows, each once, but for chips past the most a hub takes, which are those of the views made
    /// longest ago, since the newest is likeliest to be what's in front.
    pub(super) fn shown(&self) -> GithubShown {
        let mut all = GithubShown::default();
        for Shown { shown, .. } in self.views.values().rev() {
            add(&mut all.workspaces, &shown.workspaces);
            add(&mut all.closely, &shown.closely);
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

    /// Takes in `update`, telling each view that shows what it's of. Returns whether it changed anything.
    pub(super) fn apply(&mut self, update: GithubUpdate) -> bool {
        let shows: Box<dyn Fn(&GithubShown) -> bool> = match update {
            GithubUpdate::Reference { reference, read } => {
                if self.references.get(&reference) == Some(&read) {
                    return false;
                }
                self.references.insert(reference.clone(), read);
                Box::new(move |shown| shown.references.contains(&reference))
            }
            GithubUpdate::PullRequest { workspace, found, unseen } => {
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

    pub(super) fn reference(&self, ask: &ReferenceAsk) -> Option<ReferenceRead> {
        self.references.get(ask).cloned()
    }

    pub(super) fn pull_request(&self, workspace: &WorkspaceId) -> Option<PullRequestFound> {
        self.pull_requests.get(workspace).and_then(|own| own.found.clone())
    }

    pub(super) fn unseen(&self, workspace: &WorkspaceId) -> Option<RepositoryUnseen> {
        self.pull_requests.get(workspace).and_then(|own| own.unseen.clone())
    }

    /// Whether the hub has said what it knows of `workspace`'s pull request, even that it hasn't looked.
    pub(super) fn said_pull_request(&self, workspace: &WorkspaceId) -> bool {
        self.pull_requests.contains_key(workspace)
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
