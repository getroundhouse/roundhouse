//! What the store does with every kind's table alike, and the one list of every kind it keeps.

use protocol::model::{Level, Observation, ObservationKey, Rev};

use super::file::{self, Row, Write};
use super::kept::Table;
use super::kinds::{
    AccountIdentity, AccountLimits, Form, Kind, Owner, Referenced, Subject, WorkspaceChanges, WorkspaceLines,
    WorkspacePullRequest, WorkspacePullRequestDetail, WorkspaceRepositoryUnseen,
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
    pub(super) account_identity: Table<AccountIdentity>,
    pub(super) account_limits: Table<AccountLimits>,
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
    /// removal from the file on `writer`. Returns which it forgot.
    fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<ObservationKey>;
}

impl<K: Kind> AnyTable for Table<K> {
    fn form(&self) -> Form {
        K::FORM
    }

    fn listed(&self, level: &Level) -> Vec<Observation> {
        let at_level = self.kept.iter().filter(|(of, _)| K::key(of).level().as_ref() == Some(level));
        at_level.map(|(of, kept)| K::observation(of, kept.said.observed(), kept.rev)).collect()
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

    fn forget_of(&mut self, gone: Gone<'_>, writer: &Writer) -> Vec<ObservationKey> {
        self.kept_current.retain(|of| !of.owner().is_some_and(gone));
        let forgotten: Vec<K::Of> = self.kept.keys().filter(|of| of.owner().is_some_and(gone)).cloned().collect();
        for of in &forgotten {
            self.kept.remove(of);
            writer.write::<K>(of.clone(), Write::Gone);
        }
        forgotten.iter().map(K::key).collect()
    }
}

impl Tables {
    /// Each kind's table, the one list of every kind the store keeps.
    pub(super) fn each(&mut self) -> [&mut dyn AnyTable; 8] {
        let Tables {
            workspace_lines,
            workspace_changes,
            referenced,
            workspace_pull_request,
            workspace_pull_request_detail,
            workspace_repository_unseen,
            account_identity,
            account_limits,
        } = self;
        [
            workspace_lines,
            workspace_changes,
            referenced,
            workspace_pull_request,
            workspace_pull_request_detail,
            workspace_repository_unseen,
            account_identity,
            account_limits,
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

    /// The form of each kind's table in the cache file.
    pub(super) fn forms() -> Vec<Form> {
        Tables::default().each().iter().map(|table| table.form()).collect()
    }
}
