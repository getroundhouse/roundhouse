//! What the store does with every kind's table alike, and the one list of every kind it keeps.

use protocol::model::{Level, Observation, ObservationKey, RecordKey, Rev};

use super::file::{self, Row, Write};
use super::github_kinds::{GithubAppBot, GithubAppSaid, GithubProfile, GithubUsername, InstalledOn, RepositoryReach};
use super::kept::Table;
use super::kinds::{
    AccountIdentity, AccountLimits, Form, Kind, ModelCatalog, Owner, Referenced, SessionCache, SessionContext,
    SessionTodos, Subject, WaitRead, WorkspaceChanges, WorkspaceLines, WorkspacePullRequest,
    WorkspacePullRequestDetail, WorkspaceRepositoryUnseen,
};
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
    pub(super) session_cache: Table<SessionCache>,
    pub(super) wait_read: Table<WaitRead>,
    pub(super) github_profile: Table<GithubProfile>,
}

/// What's told of an observation that's forgotten: its removal, unless it's of a kind sent nowhere, and the removal of
/// the record made from it for a client of a version from before it, if any is, as [`Kind::old_record`] says.
pub(super) struct Forgotten {
    pub(super) observation: Option<ObservationKey>,
    pub(super) old_record: Option<RecordKey>,
}

/// Which owners' observations are to be forgotten, since they're gone.
pub(super) type Gone<'a> = &'a (dyn Fn(Owner<'_>) -> bool + Sync);

/// What the store does with every kind's table alike.
pub(super) trait AnyTable {
    fn form(&self) -> Form;
    /// Each observation it keeps that's sent at `level`.
    fn listed(&self, level: &Level) -> Vec<Observation>;
    /// Keeps what the cache file kept, each as of when it was read, at `rev`.
    fn load(&mut self, file: &CacheFile, rev: Rev) -> rusqlite::Result<()>;
    /// Whether it keeps an observation of an owner that `gone` picks out.
    fn keeps_of(&self, gone: Gone<'_>) -> bool;
    /// Forgets each observation of an owner that `gone` picks out, and that anything keeps it current, and queues its
    /// removal from the file on `writer`. Returns which it forgot, as clients are told of each.
    fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<Forgotten>;
}

impl<K: Kind> AnyTable for Table<K> {
    fn form(&self) -> Form {
        K::FORM
    }

    fn listed(&self, level: &Level) -> Vec<Observation> {
        let at_level =
            self.kept.iter().filter(|(of, _)| K::key(of).and_then(|key| key.level()).as_ref() == Some(level));
        at_level.filter_map(|(of, kept)| K::observation(of, kept.said.observed(), kept.rev)).collect()
    }

    fn load(&mut self, file: &CacheFile, rev: Rev) -> rusqlite::Result<()> {
        for Row { of, value, read_at_ms } in file::load::<K>(file)? {
            self.keep_loaded(of, value, read_at_ms, rev);
        }
        Ok(())
    }

    fn keeps_of(&self, gone: Gone<'_>) -> bool {
        self.kept.keys().any(|of| of.owner().is_some_and(gone))
    }

    fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<Forgotten> {
        self.kept_current.retain(|of| !of.owner().is_some_and(gone));
        let forgotten: Vec<K::Of> = self.kept.keys().filter(|of| of.owner().is_some_and(gone)).cloned().collect();
        for of in &forgotten {
            self.kept.remove(of);
            writer.write::<K>(of.clone(), Write::Gone);
        }
        let told = |of: &K::Of| Forgotten { observation: K::key(of), old_record: K::old_record_key(of) };
        forgotten.iter().map(told).collect()
    }
}

impl Tables {
    /// Each kind's table, the one list of every kind the store keeps.
    pub(super) fn each(&mut self) -> [&mut dyn AnyTable; 19] {
        let Tables {
            workspace_lines,
            workspace_changes,
            referenced,
            workspace_pull_request,
            workspace_pull_request_detail,
            workspace_repository_unseen,
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
            session_cache,
            wait_read,
            github_profile,
        } = self;
        [
            workspace_lines,
            workspace_changes,
            referenced,
            workspace_pull_request,
            workspace_pull_request_detail,
            workspace_repository_unseen,
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
            session_cache,
            wait_read,
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
    pub(super) fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<Forgotten> {
        self.each().into_iter().flat_map(|table| table.forget_of(gone, writer)).collect()
    }

    /// The form of each kind's table in the cache file.
    pub(super) fn forms() -> Vec<Form> {
        Tables::default().each().iter().map(|table| table.form()).collect()
    }
}
