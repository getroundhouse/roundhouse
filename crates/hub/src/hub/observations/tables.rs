//! What the store does with every kind's table alike, and the one list of every kind it keeps.

use protocol::client::ObservationNotice;
use protocol::model::{Level, Observation, ObservationKey, Rev};

use super::file::{self, Row, Write};
use super::github_kinds::{GithubAppBot, GithubAppSaid, GithubProfile, GithubUsername, InstalledOn, RepositoryReach};
use super::hub_kinds::ReleaseOffered;
use super::keepers::Keeper;
use super::kept::{Changed, Step, Table};
use super::kinds::{
    AccountIdentity, AccountLimits, Form, Kind, ModelCatalog, Owner, Referenced, SessionCache, SessionContext,
    SessionTodos, SessionToolServers, Subject, WaitRead, WorkspaceChanges, WorkspaceLines, WorkspacePullRequest,
    WorkspacePullRequestDetail, WorkspaceRepositoryUnseen,
};
use super::workspace_kinds::{ChildWorktreeBranches, WorkspaceBranches, WorkspaceWorkerBuild};
use super::writer::Writer;
use crate::cache_file::CacheFile;

/// Each kind's table, in memory.
#[derive(Default)]
pub(crate) struct Tables {
    pub(super) workspace_lines: Table<WorkspaceLines>,
    pub(super) workspace_changes: Table<WorkspaceChanges>,
    pub(super) referenced: Table<Referenced>,
    pub(super) workspace_pull_request: Table<WorkspacePullRequest>,
    pub(super) workspace_pull_request_detail: Table<WorkspacePullRequestDetail>,
    pub(super) workspace_repository_unseen: Table<WorkspaceRepositoryUnseen>,
    pub(super) workspace_branches: Table<WorkspaceBranches>,
    pub(super) workspace_worker_build: Table<WorkspaceWorkerBuild>,
    pub(super) repository_reach: Table<RepositoryReach>,
    pub(super) github_username: Table<GithubUsername>,
    pub(super) github_app_said: Table<GithubAppSaid>,
    pub(super) github_app_bot: Table<GithubAppBot>,
    pub(super) installed_on: Table<InstalledOn>,
    pub(super) model_catalog: Table<ModelCatalog>,
    pub(super) account_identity: Table<AccountIdentity>,
    pub(super) account_limits: Table<AccountLimits>,
    pub(super) session_context: Table<SessionContext>,
    pub(super) session_todos: Table<SessionTodos>,
    pub(super) session_tool_servers: Table<SessionToolServers>,
    pub(super) session_cache: Table<SessionCache>,
    pub(super) wait_read: Table<WaitRead>,
    pub(super) release_offered: Table<ReleaseOffered>,
    pub(super) child_worktree_branches: Table<ChildWorktreeBranches>,
    pub(super) github_profile: Table<GithubProfile>,
}

/// Which owners' observations are to be forgotten, since they're gone.
pub(super) type Gone<'a> = &'a (dyn Fn(Owner<'_>) -> bool + Sync);

/// What the store does with every kind's table alike.
pub(super) trait AnyTable {
    /// Its table in the cache file, and that table's version, or `None` for a kind kept in memory alone.
    fn form(&self) -> Option<Form>;
    /// Each observation it keeps that's sent at `level`.
    fn listed(&self, level: &Level) -> Vec<Observation>;
    /// Keeps what the cache file kept, each as of when it was read, at `rev`, which is nothing of a kind kept in memory
    /// alone.
    fn load(&mut self, file: &CacheFile, rev: Rev) -> rusqlite::Result<()>;
    /// Whether it keeps an observation of an owner that `gone` picks out.
    fn keeps_of(&self, gone: Gone<'_>) -> bool;
    /// Forgets each observation of an owner that `gone` picks out, and that anything keeps it current, and queues its
    /// removal from the file on `writer`. Returns which it forgot, as clients are told of each.
    fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<ObservationKey>;
    /// Whether `keeper` going at `at_ms` changes what any observation it keeps says, as [`AnyTable::keeper_went`] would.
    fn changes_as_it_goes(&self, keeper: Keeper, at_ms: i64) -> bool;
    /// Lets go of each observation `keeper` keeps, which is as its kind says once unkept, as of `at_ms`, at `rev`, and
    /// queues each write on `writer`. Returns what clients are to be told. With no `rev`, as where the database gives
    /// none, it only lets go of them, and each says what it said.
    fn keeper_went(&mut self, keeper: Keeper, at_ms: i64, rev: Option<Rev>, writer: &Writer) -> Vec<ObservationNotice>;
    /// Whether it keeps an observation nothing keeps, of a kind that's gone once unkept.
    fn keeps_unkept(&self) -> bool;
    /// Forgets each of those at `rev`, and queues each removal on `writer`. Returns what clients are to be told.
    fn forget_unkept(&mut self, rev: Rev, writer: &Writer) -> Vec<ObservationNotice>;
}

impl<K: Kind> AnyTable for Table<K> {
    fn form(&self) -> Option<Form> {
        (!K::IN_MEMORY_ALONE).then_some(K::FORM)
    }

    fn listed(&self, level: &Level) -> Vec<Observation> {
        let at_level =
            self.kept.iter().filter(|(of, _)| K::key(of).and_then(|key| key.level()).as_ref() == Some(level));
        at_level.filter_map(|(of, kept)| K::observation(of, kept.said.observed(), kept.rev)).collect()
    }

    fn load(&mut self, file: &CacheFile, rev: Rev) -> rusqlite::Result<()> {
        if K::IN_MEMORY_ALONE {
            return Ok(());
        }
        for Row { of, value, read_at_ms } in file::load::<K>(file)? {
            self.keep_loaded(of, value, read_at_ms, rev);
        }
        Ok(())
    }

    fn keeps_of(&self, gone: Gone<'_>) -> bool {
        self.kept.keys().any(|of| of.owner().is_some_and(gone))
    }

    fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<ObservationKey> {
        self.keepers.retain(|of, _| !of.owner().is_some_and(gone));
        let forgotten: Vec<K::Of> = self.kept.keys().filter(|of| of.owner().is_some_and(gone)).cloned().collect();
        for of in &forgotten {
            self.kept.remove(of);
            writer.write::<K>(of.clone(), Write::Gone);
        }
        forgotten.iter().filter_map(K::key).collect()
    }

    fn changes_as_it_goes(&self, keeper: Keeper, at_ms: i64) -> bool {
        let unkept = |of: &K::Of| self.unkept(of, K::UNKEPT.at(at_ms));
        self.kept_by_keeper(keeper).iter().any(|of| matches!(unkept(of), Step::Changed(_)))
    }

    fn keeper_went(&mut self, keeper: Keeper, at_ms: i64, rev: Option<Rev>, writer: &Writer) -> Vec<ObservationNotice> {
        let mut notices = Vec::new();
        for of in self.kept_by_keeper(keeper) {
            self.keepers.remove(&of);
            if let (Some(rev), Step::Changed(changed)) = (rev, self.unkept(&of, K::UNKEPT.at(at_ms))) {
                notices.extend(self.change(&of, changed, rev, writer));
            }
        }
        notices
    }

    fn keeps_unkept(&self) -> bool {
        !self.unkept_of_a_kind_gone_once_unkept().is_empty()
    }

    fn forget_unkept(&mut self, rev: Rev, writer: &Writer) -> Vec<ObservationNotice> {
        let unkept = self.unkept_of_a_kind_gone_once_unkept();
        unkept.iter().filter_map(|of| self.change(of, Changed::Gone, rev, writer)).collect()
    }
}

impl Tables {
    /// Each kind's table, the one list of every kind the store keeps.
    pub(super) fn each(&mut self) -> [&mut dyn AnyTable; 24] {
        let Tables {
            workspace_lines,
            workspace_changes,
            referenced,
            workspace_pull_request,
            workspace_pull_request_detail,
            workspace_repository_unseen,
            workspace_branches,
            workspace_worker_build,
            repository_reach,
            github_username,
            github_app_said,
            github_app_bot,
            installed_on,
            model_catalog,
            account_identity,
            account_limits,
            session_context,
            session_todos,
            session_tool_servers,
            session_cache,
            wait_read,
            release_offered,
            child_worktree_branches,
            github_profile,
        } = self;
        [
            workspace_lines,
            workspace_changes,
            referenced,
            workspace_pull_request,
            workspace_pull_request_detail,
            workspace_repository_unseen,
            workspace_branches,
            workspace_worker_build,
            repository_reach,
            github_username,
            github_app_said,
            github_app_bot,
            installed_on,
            model_catalog,
            account_identity,
            account_limits,
            session_context,
            session_todos,
            session_tool_servers,
            session_cache,
            wait_read,
            release_offered,
            child_worktree_branches,
            github_profile,
        ]
    }

    /// Every observation sent at `level`, of every kind.
    pub(super) fn listed(&mut self, level: &Level) -> Vec<Observation> {
        self.each().iter().flat_map(|table| table.listed(level)).collect()
    }

    /// Whether any kind keeps an observation of an owner that `gone` picks out.
    pub(super) fn keep_of(&mut self, gone: Gone<'_>) -> bool {
        self.each().iter().any(|table| table.keeps_of(gone))
    }

    /// Forgets every observation of an owner that `gone` picks out, of every kind, as [`AnyTable::forget_of`] does.
    pub(super) fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<ObservationKey> {
        self.each().into_iter().flat_map(|table| table.forget_of(gone, writer)).collect()
    }

    /// Whether `keeper` going at `at_ms` changes anything of any kind, as [`AnyTable::changes_as_it_goes`] says.
    pub(super) fn change_as_it_goes(&mut self, keeper: Keeper, at_ms: i64) -> bool {
        self.each().iter().any(|table| table.changes_as_it_goes(keeper, at_ms))
    }

    /// Lets go of every observation `keeper` keeps, of every kind, as [`AnyTable::keeper_went`] does.
    pub(super) fn keeper_went(
        &mut self,
        keeper: Keeper,
        at_ms: i64,
        rev: Option<Rev>,
        writer: &Writer,
    ) -> Vec<ObservationNotice> {
        self.each().into_iter().flat_map(|table| table.keeper_went(keeper, at_ms, rev, writer)).collect()
    }

    /// Whether any kind keeps what [`AnyTable::forget_unkept`] forgets.
    pub(super) fn keep_unkept(&mut self) -> bool {
        self.each().iter().any(|table| table.keeps_unkept())
    }

    /// Forgets what nothing keeps of every kind that's gone once unkept, as [`AnyTable::forget_unkept`] does.
    pub(super) fn forget_unkept(&mut self, rev: Rev, writer: &Writer) -> Vec<ObservationNotice> {
        self.each().into_iter().flat_map(|table| table.forget_unkept(rev, writer)).collect()
    }

    /// The form of each kind's table in the cache file, of each kind not kept in memory alone.
    pub(super) fn forms() -> Vec<Form> {
        Tables::default().each().iter().filter_map(|table| table.form()).collect()
    }
}
