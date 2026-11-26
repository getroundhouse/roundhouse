//! What the store keeps of each observation in memory, and what a change to one makes of it.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ObservationNotice};
use protocol::model::{
    AccountId, ChildWorktreeId, GithubAccountId, GithubAppId, Level, Observation, Observed, RepoId, Rev, SessionId,
    WaitId, WorkspaceId,
};
use tokio::sync::Mutex as AsyncMutex;
use tracing::warn;

use super::kinds::{Kind, Owner};
use super::tables::Tables;
use super::writer::Writer;
use crate::cache_file::CacheFile;
use crate::store::{Store, StoreError};

/// Every observation the hub holds, in memory, and in the cache file.
pub(crate) struct Observations {
    /// Held while an observation changes, from before it takes its revision until its notice is sent, and while every
    /// record is listed, so a list has every observation whose revision it takes in. An observation and a record are
    /// in different files, so a change can't take its revision in a record's transaction. A change queues its write
    /// to the file under it, and the write is made after, so no list waits on the file.
    pub(super) changing: AsyncMutex<()>,
    pub(super) tables: Mutex<Tables>,
    pub(super) writer: Writer,
}

/// What's kept of one kind of observation.
pub(crate) struct Table<K: Kind> {
    pub(super) kept: HashMap<K::Of, Kept<K::Value>>,
    /// What something keeps up to date now, such as a worker watching a worktree, so what's read of it is current.
    pub(super) kept_current: HashSet<K::Of>,
    /// Whether something keeps every observation of the kind up to date now, as the hub's own reads on a schedule do,
    /// such as of every GitHub account's username each day, so what's read of any is current, whatever it's of.
    pub(super) all_kept_current: bool,
}

impl<K: Kind> Default for Table<K> {
    fn default() -> Self {
        Self { kept: HashMap::new(), kept_current: HashSet::new(), all_kept_current: false }
    }
}

/// One observation, as its revision says it.
pub(super) struct Kept<V> {
    pub(super) said: Said<V>,
    pub(super) rev: Rev,
}

/// What an observation says, and when it was read.
#[derive(Clone)]
pub(super) struct Said<V> {
    pub(super) value: V,
    /// Whether it's read as something keeps it up to date.
    current: bool,
    /// When it was read, as clients are told while it isn't current: the read that found what it says, or, once
    /// nothing keeps it current, when nothing did any more. A later read that finds the same moves no revision, so it moves this
    /// neither, and every client of one revision is told the same.
    as_of_ms: i64,
    /// When it was last read, which the cache file keeps.
    pub(super) read_at_ms: i64,
}

impl<V: Clone> Said<V> {
    pub(super) fn observed(&self) -> Observed<V> {
        let value = self.value.clone();
        if self.current { Observed::Current { value } } else { Observed::AsOf { value, as_of_ms: self.as_of_ms } }
    }
}

/// A change the code that reads something makes to an observation.
pub(super) enum Change<V> {
    /// It was read, and found `value`.
    Read { value: V, read_at_ms: i64 },
    /// It was worked out now, from what was read at `as_of_ms`, and says `value`.
    WorkedOut { value: V, as_of_ms: i64 },
    /// Part of it was heard at `heard_at_ms`, and `value` is what's kept merged with that part, as an account's limits
    /// are, each kept as it was last heard. It's as of the newest hearing, and never dropped as a late read, since the
    /// merge keeps each part as it was last heard, however late a hearing lands.
    Merged { value: V, heard_at_ms: i64 },
    /// Something keeps it up to date from now on, so what's read of it from now is current.
    KeptCurrent,
    /// Nothing keeps it up to date any more, since `as_of_ms`, so it's as of then: what kept it up to date would have
    /// said of any change before. As of its last read where that's later, as one that landed since can be.
    LetGo { as_of_ms: i64 },
    /// Nothing reads it again to keep it up to date any more, so it's as of its last read, since nothing would have
    /// said of a change since.
    NoLongerReadAgain,
    /// What it's of is gone.
    Gone,
}

/// What a change makes of an observation, worked out before it takes a revision.
pub(super) enum Step<V> {
    /// Nothing to keep or tell.
    Same,
    /// It says what it said, read again since, which the file keeps. Nothing is told, and its revision stays.
    Reread(Said<V>),
    /// It says what it said, as its kind compares what it says, but holds something newer that clients aren't told of,
    /// such as when each of an account's limits was heard, which memory and the file keep. Nothing is told, and its
    /// revision stays.
    Reheard(Said<V>),
    /// It says something new, or is gone, under a new revision.
    Changed(Changed<V>),
}

/// What changes under a new revision.
pub(super) enum Changed<V> {
    Says(Said<V>),
    Gone,
}

impl<K: Kind> Table<K> {
    /// What `change` makes of the observation of `of`. Whether something keeps it current changes here, since that's
    /// told of only through what's read.
    pub(super) fn step(&mut self, of: &K::Of, change: Change<K::Value>) -> Step<K::Value> {
        let kept = self.kept.get(of).map(|kept| &kept.said);
        match change {
            Change::Read { value, read_at_ms } => {
                let current = self.kept_current(of);
                match kept {
                    // A read that began before the one kept, and lands after it, as a slow one can.
                    Some(kept) if kept.read_at_ms > read_at_ms => Step::Same,
                    Some(kept) if kept.value == value && kept.current == current => {
                        Step::Reread(Said { read_at_ms, ..kept.clone() })
                    }
                    _ => Step::Changed(Changed::Says(Said { value, current, as_of_ms: read_at_ms, read_at_ms })),
                }
            }
            Change::WorkedOut { value, as_of_ms } => {
                let current = self.kept_current(of);
                match kept {
                    Some(kept) if kept.value == value && kept.current == current => {
                        if kept.read_at_ms == as_of_ms {
                            Step::Same
                        } else {
                            Step::Reread(Said { read_at_ms: as_of_ms, ..kept.clone() })
                        }
                    }
                    _ => Step::Changed(Changed::Says(Said { value, current, as_of_ms, read_at_ms: as_of_ms })),
                }
            }
            Change::Merged { value, heard_at_ms } => self.merged(of, value, heard_at_ms),
            Change::KeptCurrent => {
                // What was read before isn't current, so it's not said to be until the next read.
                self.kept_current.insert(of.clone());
                Step::Same
            }
            Change::LetGo { as_of_ms } => {
                self.kept_current.remove(of);
                match kept {
                    Some(kept) if kept.current => {
                        let at = as_of_ms.max(kept.read_at_ms);
                        Step::Changed(Changed::Says(Said {
                            current: false,
                            as_of_ms: at,
                            read_at_ms: at,
                            ..kept.clone()
                        }))
                    }
                    Some(_) | None => Step::Same,
                }
            }
            Change::NoLongerReadAgain => {
                self.kept_current.remove(of);
                match kept {
                    Some(kept) if kept.current => {
                        Step::Changed(Changed::Says(Said { current: false, as_of_ms: kept.read_at_ms, ..kept.clone() }))
                    }
                    Some(_) | None => Step::Same,
                }
            }
            Change::Gone => {
                self.kept_current.remove(of);
                if kept.is_some() { Step::Changed(Changed::Gone) } else { Step::Same }
            }
        }
    }

    /// Whether something keeps the observation of `of` up to date now, on its own or with every one of its kind.
    pub(super) fn kept_current(&self, of: &K::Of) -> bool {
        self.all_kept_current || self.kept_current.contains(of)
    }

    /// What a [`Change::Merged`] of `value`, heard at `heard_at_ms`, makes of the observation of `of`.
    fn merged(&self, of: &K::Of, value: K::Value, heard_at_ms: i64) -> Step<K::Value> {
        let (kept, current) = (self.kept.get(of).map(|kept| &kept.said), self.kept_current.contains(of));
        let at = kept.map_or(heard_at_ms, |kept| kept.read_at_ms.max(heard_at_ms));
        match kept {
            Some(kept) if kept.value == value && kept.read_at_ms == at => Step::Same,
            Some(kept) if K::says_the_same(&kept.value, &value) && kept.current == current => {
                Step::Reheard(Said { value, read_at_ms: at, ..kept.clone() })
            }
            _ => Step::Changed(Changed::Says(Said { value, current, as_of_ms: at, read_at_ms: at })),
        }
    }

    /// What the observation of `of` says, however fresh it is.
    pub(super) fn value(&self, of: &K::Of) -> Option<&K::Value> {
        self.said(of).map(|said| &said.value)
    }

    /// What the observation of `of` says, and when it was read.
    pub(super) fn said(&self, of: &K::Of) -> Option<&Said<K::Value>> {
        self.kept.get(of).map(|kept| &kept.said)
    }

    /// Keeps that the observation of `of` says `value`, as of `read_at_ms`, at `rev`, as the cache file kept it, since
    /// nothing keeps it up to date yet.
    pub(super) fn keep_loaded(&mut self, of: K::Of, value: K::Value, read_at_ms: i64, rev: Rev) {
        let said = Said { value, current: false, as_of_ms: read_at_ms, read_at_ms };
        self.kept.insert(of, Kept { said, rev });
    }

    /// Keeps when the observation of `of` was read again, as `said` says, under the revision it has.
    pub(super) fn reread(&mut self, of: &K::Of, said: Said<K::Value>) {
        if let Some(kept) = self.kept.get_mut(of) {
            kept.said = said;
        }
    }

    /// Keeps what `changed` made of the observation of `of`, at `rev`, and returns what clients are to be told of it,
    /// which is nothing for a kind sent nowhere.
    pub(super) fn change(&mut self, of: &K::Of, changed: Changed<K::Value>, rev: Rev) -> Option<ObservationNotice> {
        match changed {
            Changed::Says(said) => {
                let observation = K::observation(of, said.observed(), rev);
                self.kept.insert(of.clone(), Kept { said, rev });
                observation.map(ObservationNotice::Copy)
            }
            Changed::Gone => {
                self.kept.remove(of);
                K::key(of).map(|observation| ObservationNotice::Removed { observation, rev })
            }
        }
    }
}

impl Observations {
    /// The store kept in `file`, opened with [`PART`](super::PART), with each observation it kept from before as of
    /// when it was read, under one revision newer than any a client holds, since a client of the hub before can hold
    /// any revision that hub gave. Where there's no revision to give, as when the database fails, it starts with none
    /// of them, as the log says, and each is not known until it's read again. Those of a workspace, another worktree, a session, an
    /// agent's account, a repository, a GitHub account, an app of your own or an open wait the database doesn't have are forgotten, as one
    /// a hub stopped before forgetting, or a file refused to, leaves, so no client is listed them.
    pub(crate) fn open(file: Arc<CacheFile>, store: &Store) -> Self {
        let mut tables = Tables::default();
        match store.next_rev() {
            Ok(rev) => {
                for table in tables.each() {
                    if let Err(error) = table.load(&file, rev) {
                        let table = table.form().map(|form| form.table);
                        warn!(
                            name: "observation.unread",
                            table,
                            %error,
                            "Couldn't read observations from the cache's file"
                        );
                    }
                }
            }
            Err(error) => {
                warn!(
                    name: "observation.revision_unread",
                    %error,
                    "Leaving the observations in the cache's file unread, with no revision for them"
                );
            }
        }
        let writer = Writer::new(file);
        match owners(store) {
            Ok(Owners { workspaces, child_worktrees, sessions, waits, accounts, repos, github_accounts, apps }) => {
                let gone = |owner: Owner<'_>| match owner {
                    Owner::Workspace(id) => !workspaces.contains(id),
                    Owner::ChildWorktree(key) => !child_worktrees.contains(&key.id),
                    Owner::Session(key) => !sessions.contains(&key.session_id),
                    Owner::Wait(key) => !waits.contains(&key.wait),
                    Owner::Account(id) => !accounts.contains(id),
                    Owner::Repo(id) => !repos.contains(id),
                    Owner::GithubAccount(id) => !github_accounts.contains(id),
                    Owner::App(id) => !apps.contains(id),
                    // Never gone: what it owns goes only as the hub lets go of it.
                    Owner::Hub => false,
                };
                tables.forget_of(&gone, &writer);
            }
            // The task that works out GitHub's observations forgets those of workspaces, and their sessions', as it
            // starts.
            Err(error) => {
                warn!(
                    name: "observation.database_unsure",
                    %error,
                    "Keeping observations of what the database may not have"
                );
            }
        }
        Self { changing: AsyncMutex::new(()), tables: Mutex::new(tables), writer }
    }

    /// The store kept in the cache file `cache.db` in `dir`, as [`Observations::open`] opens it, for a test of what reads
    /// it that has no hub.
    #[cfg(test)]
    pub(crate) fn in_a_test(dir: &std::path::Path, store: &Store) -> Arc<Self> {
        let file = CacheFile::open(&dir.join("cache.db"), &[&super::PART]);
        Arc::new(Self::open(Arc::new(file), store))
    }

    /// Lists the records of `level` along with `list`, and every observation the hub sends at that level, under the
    /// lock a change takes, so the list's revision takes in none it lacks.
    pub(crate) async fn listed_with<T>(&self, level: &Level, list: impl Future<Output = T>) -> (T, Vec<Observation>) {
        let _changing = self.changing.lock().await;
        let listed = list.await;
        (listed, self.tables.lock().listed(level))
    }

    /// Every observation of `level`, which isn't the hub's, with the hub's newest revision, `rev_now`, read under the
    /// lock every change takes, so the revision covers no change the list lacks. A record that changes meanwhile moves
    /// the revision too, which costs nothing, since a client compares only this level's copies with the list.
    pub(crate) async fn listed_at(
        &self,
        level: &Level,
        rev_now: impl Future<Output = Result<Rev, ApiError>>,
    ) -> Result<(Vec<Observation>, Rev), ApiError> {
        let _changing = self.changing.lock().await;
        let rev = rev_now.await?;
        Ok((self.tables.lock().listed(level), rev))
    }

    /// Whether the store keeps an observation of kind `K` of `of`.
    pub(crate) fn keeps<K: Kind>(&self, of: &K::Of) -> bool {
        K::table(&mut self.tables.lock()).kept.contains_key(of)
    }

    /// What the store keeps of the observation of kind `K` of `of`, however fresh it is.
    pub(crate) fn value_of<K: Kind>(&self, of: &K::Of) -> Option<K::Value> {
        K::table(&mut self.tables.lock()).kept.get(of).map(|kept| kept.said.value.clone())
    }

    /// What the store keeps of each observation of kind `K`, by what it's of, however fresh each is.
    pub(crate) fn values_of<K: Kind>(&self) -> HashMap<K::Of, K::Value> {
        K::table(&mut self.tables.lock()).kept.iter().map(|(of, kept)| (of.clone(), kept.said.value.clone())).collect()
    }

    /// When the observation of kind `K` of `of` was last read, however fresh it is, or `None` while it isn't known, for
    /// what reads it again on a schedule of its own.
    pub(crate) fn read_at<K: Kind>(&self, of: &K::Of) -> Option<i64> {
        K::table(&mut self.tables.lock()).kept.get(of).map(|kept| kept.said.read_at_ms)
    }

    /// Keeps that the observation of kind `K` of `of` was read at `read_at_ms` and found `value`, as of then, under the
    /// first revision, as if kept before anything else, telling nobody, for a test of what reads it that has no hub to
    /// keep it, as one of `crate::github_logins`'s, or that sets up a hub before it's served.
    #[cfg(test)]
    pub(crate) fn keep_as_read<K: Kind>(&self, of: K::Of, value: K::Value, read_at_ms: i64) {
        K::table(&mut self.tables.lock()).keep_loaded(of, value, read_at_ms, Rev::new(1));
    }

    /// Forgets the observation of kind `K` of `of`, telling nobody, for a test of what reads it that has no hub, as
    /// [`Observations::keep_as_read`] keeps one.
    #[cfg(test)]
    pub(crate) fn forget_for_a_test<K: Kind>(&self, of: &K::Of) {
        K::table(&mut self.tables.lock()).kept.remove(of);
    }

    /// What each observation of kind `K` the store keeps is of.
    pub(crate) fn kept_of<K: Kind>(&self) -> Vec<K::Of> {
        K::table(&mut self.tables.lock()).kept.keys().cloned().collect()
    }
}

/// Each owner the database has, of each kind an observation can be of but the hub.
struct Owners {
    workspaces: HashSet<WorkspaceId>,
    /// Those removed too, whose branches are kept with their workspace.
    child_worktrees: HashSet<ChildWorktreeId>,
    sessions: HashSet<SessionId>,
    waits: HashSet<WaitId>,
    accounts: HashSet<AccountId>,
    repos: HashSet<RepoId>,
    github_accounts: HashSet<GithubAccountId>,
    apps: HashSet<GithubAppId>,
}

/// Each owner `store` has, as [`Observations::open`] forgets the observations of the rest.
fn owners(store: &Store) -> Result<Owners, StoreError> {
    Ok(Owners {
        workspaces: store.list_workspaces(true)?.into_iter().map(|workspace| workspace.id).collect(),
        child_worktrees: store.child_worktree_ids()?,
        sessions: store.session_ids()?,
        waits: store.open_wait_ids()?,
        accounts: store.account_ids()?,
        repos: store.list_repos()?.into_iter().map(|repo| repo.id).collect(),
        github_accounts: store.github_account_ids()?,
        apps: store.github_app_ids()?,
    })
}
