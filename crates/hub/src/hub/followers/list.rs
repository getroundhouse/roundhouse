use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use protocol::model::{GithubRepositoryName, IssueNumber, WorkspaceId};

use super::{Followed, FollowerId};
use crate::github_cache::Item;
use crate::hub::Hub;

/// Who follows what, by what's followed. A terminal's worker sends what it shows, and the hub reads a workspace's pull
/// request or a chip's issue more often, only while something here follows it.
#[derive(Default)]
pub(crate) struct Followers(Mutex<HashMap<Followed, HashSet<FollowerId>>>);

impl Followers {
    fn add<'a>(&self, by: FollowerId, of: impl IntoIterator<Item = &'a Followed>) {
        let mut followers = self.0.lock();
        for followed in of {
            followers.entry(followed.clone()).or_default().insert(by);
        }
    }

    fn remove<'a>(&self, by: FollowerId, of: impl IntoIterator<Item = &'a Followed>) {
        let mut followers = self.0.lock();
        for followed in of {
            if let Some(by_now) = followers.get_mut(followed) {
                by_now.remove(&by);
                if by_now.is_empty() {
                    followers.remove(followed);
                }
            }
        }
    }

    /// Whether any subscription follows it.
    pub(crate) fn follows(&self, followed: &Followed) -> bool {
        self.0.lock().contains_key(followed)
    }

    /// The workspaces some subscription follows.
    pub(crate) fn workspaces(&self) -> HashSet<WorkspaceId> {
        let followers = self.0.lock();
        let workspace = |followed: &Followed| match followed {
            Followed::Workspace(id) => Some(id.clone()),
            Followed::Hub | Followed::Session(_) | Followed::Terminal(_) | Followed::Reference(_) => None,
        };
        followers.keys().filter_map(workspace).collect()
    }

    /// The issues and pull requests some subscription follows, each once, as GitHub tells them apart, whatever the
    /// case of their repositories' names.
    pub(crate) fn references(&self) -> Vec<(GithubRepositoryName, IssueNumber)> {
        let followers = self.0.lock();
        let (mut items, mut all) = (HashSet::new(), Vec::new());
        for followed in followers.keys() {
            match followed {
                Followed::Reference(ask) if items.insert(Item::new(&ask.repository, ask.number)) => {
                    all.push((ask.repository.clone(), ask.number));
                }
                Followed::Reference(_)
                | Followed::Hub
                | Followed::Workspace(_)
                | Followed::Session(_)
                | Followed::Terminal(_) => {}
            }
        }
        all
    }
}

/// One subscription's entries in the hub's list of who follows what, which it takes out as it's dropped: when the
/// subscription stops, or its connection ends.
pub(crate) struct Following {
    hub: Weak<Hub>,
    by: FollowerId,
    of: HashSet<Followed>,
}

impl Following {
    /// Follows `of` in place of what it followed, as a `GithubSubscribe` sent again under its ID does.
    pub(crate) fn replace(&mut self, of: impl IntoIterator<Item = Followed>) {
        let of: HashSet<Followed> = of.into_iter().collect();
        let Some(hub) = self.hub.upgrade() else { return };
        let newly: Vec<Followed> = of.difference(&self.of).cloned().collect();
        let gone: Vec<Followed> = self.of.difference(&of).cloned().collect();
        hub.followers.add(self.by, &newly);
        hub.followers.remove(self.by, &gone);
        self.of = of;
        hub.newly_followed(&newly);
        hub.no_longer_followed(&gone);
    }
}

impl Drop for Following {
    fn drop(&mut self) {
        let Some(hub) = self.hub.upgrade() else { return };
        let of: Vec<Followed> = self.of.drain().collect();
        // Taken out first, so what's done next never counts it.
        hub.followers.remove(self.by, &of);
        hub.no_longer_followed(&of);
    }
}

impl Hub {
    /// Whether any subscription follows it, for a test of what ends a following.
    #[cfg(test)]
    pub(crate) fn followed(&self, followed: &Followed) -> bool {
        self.followers.follows(followed)
    }

    /// Has `by` follow each of `of` until what this returns is dropped.
    pub(crate) fn follow(self: &Arc<Self>, by: FollowerId, of: impl IntoIterator<Item = Followed>) -> Following {
        let mut following = Following { hub: Arc::downgrade(self), by, of: HashSet::new() };
        following.replace(of);
        following
    }

    /// Does what's to be done now that a subscription follows each of `of`: a terminal's worker is told, if no other
    /// follower has had it told, and a workspace's pull request, or an issue or a pull request a chip shows, is read,
    /// as a client asking for it would have it read.
    fn newly_followed(self: &Arc<Self>, of: &[Followed]) {
        let (mut references, mut workspaces) = (Vec::new(), Vec::new());
        for followed in of {
            match followed {
                Followed::Terminal(id) => self.terminal_watching_soon(id),
                Followed::Workspace(id) => workspaces.push(id.clone()),
                Followed::Reference(ask) => references.push(ask.clone()),
                Followed::Hub | Followed::Session(_) => {}
            }
        }
        self.read_newly_followed(references, workspaces);
    }

    /// Does what's to be done now that a subscription no longer follows each of `of`: a terminal's worker is told,
    /// once nothing follows the terminal. What's read of GitHub is read less often by itself.
    fn no_longer_followed(self: &Arc<Self>, of: &[Followed]) {
        for followed in of {
            match followed {
                Followed::Terminal(id) => self.terminal_watching_soon(id),
                Followed::Hub | Followed::Workspace(_) | Followed::Session(_) | Followed::Reference(_) => {}
            }
        }
    }
}
