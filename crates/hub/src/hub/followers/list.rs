use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use parking_lot::Mutex;
use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk, WorkspaceId};
use tokio::sync::watch;

use super::Followed;
use crate::github_cache::Item;
use crate::hub::Hub;

/// Who follows what, by what's followed. A terminal's worker sends what it shows, and the hub reads a workspace's pull
/// request or a chip's issue more often, only while something here follows it.
pub(crate) struct Followers {
    /// How many subscriptions follow each that any follows. Each [`Following`] adds what it newly follows, and takes
    /// out what it no longer does, once, so the count is of subscriptions.
    by: Mutex<HashMap<Followed, usize>>,
    /// Told as [`Hub::newly_followed`] and [`Hub::no_longer_followed`] act on a change to what's followed, for what works
    /// out GitHub's observations to read the list again, as a workspace's watch does.
    changed: watch::Sender<()>,
}

impl Default for Followers {
    fn default() -> Self {
        Self { by: Mutex::default(), changed: watch::Sender::new(()) }
    }
}

impl Followers {
    fn add<'a>(&self, of: impl IntoIterator<Item = &'a Followed>) {
        let mut followers = self.by.lock();
        for followed in of {
            *followers.entry(followed.clone()).or_default() += 1;
        }
    }

    fn remove<'a>(&self, of: impl IntoIterator<Item = &'a Followed>) {
        let mut followers = self.by.lock();
        for followed in of {
            if let Some(count) = followers.get_mut(followed) {
                *count -= 1;
                if *count == 0 {
                    followers.remove(followed);
                }
            }
        }
    }

    /// Tells each time what's followed has changed, from now on, once what acts on it does, as
    /// [`Hub::newly_followed`] says.
    pub(crate) fn changes(&self) -> watch::Receiver<()> {
        self.changed.subscribe()
    }

    /// The issues and pull requests some subscription follows, each as it's asked for, so two that name one in another
    /// case are both here.
    pub(crate) fn reference_asks(&self) -> HashSet<ReferenceAsk> {
        let followers = self.by.lock();
        let ask = |followed: &Followed| match followed {
            Followed::Reference(ask) => Some(ask.clone()),
            Followed::Hub | Followed::Workspace(_) | Followed::Session(_) | Followed::Terminal(_) => None,
        };
        followers.keys().filter_map(ask).collect()
    }

    /// Whether any subscription follows it.
    pub(crate) fn follows(&self, followed: &Followed) -> bool {
        self.by.lock().contains_key(followed)
    }

    /// The workspaces some subscription follows.
    pub(crate) fn workspaces(&self) -> HashSet<WorkspaceId> {
        let followers = self.by.lock();
        let workspace = |followed: &Followed| match followed {
            Followed::Workspace(id) => Some(id.clone()),
            Followed::Hub | Followed::Session(_) | Followed::Terminal(_) | Followed::Reference(_) => None,
        };
        followers.keys().filter_map(workspace).collect()
    }

    /// The issues and pull requests some subscription follows, each once, as GitHub tells them apart, whatever the
    /// case of their repositories' names.
    pub(crate) fn references(&self) -> Vec<(GithubRepositoryName, IssueNumber)> {
        let followers = self.by.lock();
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
    of: HashSet<Followed>,
}

impl Following {
    /// Follows `of` in place of what it followed, as a `GithubSubscribe` sent again under its ID does.
    pub(crate) fn replace(&mut self, of: impl IntoIterator<Item = Followed>) {
        let of: HashSet<Followed> = of.into_iter().collect();
        let Some(hub) = self.hub.upgrade() else { return };
        let newly: Vec<Followed> = of.difference(&self.of).cloned().collect();
        let gone: Vec<Followed> = self.of.difference(&of).cloned().collect();
        hub.followers.add(&newly);
        hub.followers.remove(&gone);
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
        hub.followers.remove(&of);
        hub.no_longer_followed(&of);
    }
}

impl Hub {
    /// Whether any subscription follows it, for a test of what ends a following.
    #[cfg(test)]
    pub(crate) fn followed(&self, followed: &Followed) -> bool {
        self.followers.follows(followed)
    }

    /// Has a subscription follow each of `of` until what this returns is dropped.
    pub(crate) fn follow(self: &Arc<Self>, of: impl IntoIterator<Item = Followed>) -> Following {
        let mut following = Following { hub: Arc::downgrade(self), of: HashSet::new() };
        following.replace(of);
        following
    }

    /// Does what's to be done now that a subscription follows each of `of`: a terminal's worker is told, if no other
    /// follower has had it told, as is a workspace's, to watch its worktree, and a workspace's pull request, or an issue
    /// or a pull request a chip shows, is read, as a client asking for it would have it read. And what works out GitHub's
    /// observations is told, to read the list again. Each reads the list for what's followed now, rather than going by
    /// `of`, so what's done last is what's so once followers stop coming and going.
    fn newly_followed(self: &Arc<Self>, of: &[Followed]) {
        if !of.is_empty() {
            self.followers.changed.send_replace(());
        }
        let (mut references, mut workspaces) = (Vec::new(), Vec::new());
        for followed in of {
            match followed {
                Followed::Terminal(id) => self.terminal_watching_soon(id),
                Followed::Workspace(id) => {
                    self.changes_watching_soon(id);
                    workspaces.push(id.clone());
                }
                Followed::Reference(ask) => references.push(ask.clone()),
                Followed::Hub | Followed::Session(_) => {}
            }
        }
        self.read_newly_followed(references, workspaces);
    }

    /// Does what's to be done now that a subscription no longer follows each of `of`: a terminal's worker is told,
    /// once nothing follows the terminal, and so is a workspace's, once nothing follows the workspace. What's read of
    /// GitHub is read less often by itself, and what works out GitHub's observations is told, to read the list again.
    fn no_longer_followed(self: &Arc<Self>, of: &[Followed]) {
        if !of.is_empty() {
            self.followers.changed.send_replace(());
        }
        for followed in of {
            match followed {
                Followed::Terminal(id) => self.terminal_watching_soon(id),
                Followed::Workspace(id) => self.changes_watching_soon(id),
                Followed::Hub | Followed::Session(_) | Followed::Reference(_) => {}
            }
        }
    }
}
