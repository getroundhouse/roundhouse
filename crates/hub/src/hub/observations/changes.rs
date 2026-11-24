//! How the code that reads something changes what the store keeps of it, and the one place that tells clients.

use std::collections::HashSet;

use protocol::client::{ApiError, ObservationNotice};
use protocol::model::{GithubAccountId, GithubAppId, WorkspaceId};
use tracing::warn;

use super::file::Write;
use super::kept::{Change, Changed, Step, Table};
use super::kinds::{Kind, Owner, SessionKey};
use super::tables::Gone;
use crate::hub::Hub;
use crate::store::Store;

impl Hub {
    /// Keeps that the observation of `of` was read at `read_at_ms`, by the hub's clock, and found `value`. It's current
    /// if something keeps it up to date, as [`Hub::observation_kept_current`] says, and otherwise as of `read_at_ms`. A
    /// read that finds what's kept tells clients nothing and moves no revision, and the file keeps only that it was
    /// read then, so a hub that starts again has it as of this read. One that began before the read kept, and lands
    /// after it, is dropped.
    pub(crate) async fn observation_read<K: Kind>(
        &self,
        of: K::Of,
        value: K::Value,
        read_at_ms: i64,
    ) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::Read { value, read_at_ms }).await
    }

    /// Keeps a read, as [`Hub::observation_read`] does, if `wanted` says it's still wanted, which it's asked under the
    /// lock every change takes. So whatever stops wanting it, and then forgets what's kept of it, as
    /// [`Hub::observations_gone_where`] does, finds the read either kept by then, and forgets it, or never made, as when
    /// the hub lets go of a list of models while its agent's answer lands.
    pub(crate) async fn observation_read_while<K: Kind>(
        &self,
        of: K::Of,
        value: K::Value,
        read_at_ms: i64,
        wanted: impl FnOnce() -> bool,
    ) -> Result<(), ApiError> {
        self.change_observation_while::<K>(of, wanted, |_| Change::Read { value, read_at_ms }).await
    }

    /// Keeps what `merge` makes of what the store keeps of the observation of `of`, or of nothing where it keeps none,
    /// with what was heard at `heard_at_ms`, for a kind whose every hearing says only part of it, such as what an agent
    /// hears of its account's limits, which can leave out what hasn't changed. Under the lock every change takes, so of
    /// two at once, each merges what the other left. It's as of the newest hearing, and a hearing that lands late isn't
    /// dropped, since `merge` keeps each part as it was last heard.
    pub(crate) async fn observation_updated<K: Kind>(
        &self,
        of: K::Of,
        heard_at_ms: i64,
        merge: impl FnOnce(Option<&K::Value>) -> K::Value,
    ) -> Result<(), ApiError> {
        let held = of.clone();
        self.change_observation::<K>(of, |table| Change::Merged { value: merge(table.value(&held)), heard_at_ms }).await
    }

    /// Keeps that the observation of `of`, worked out now from what it's worked out from, says `value`, as of
    /// `as_of_ms`, when what it's worked out from was read. It's current if something keeps it up to date, as
    /// [`Hub::observation_kept_current`] says, and otherwise as of `as_of_ms`. Unlike [`Hub::observation_read`], it
    /// drops nothing as a late read, since a value worked out from what's kept now is never older than one worked out
    /// before, even where what it's worked out from was read earlier, as when a workspace's pull request becomes
    /// another. So whatever works out a kind works each out in turn.
    pub(crate) async fn observation_worked_out<K: Kind>(
        &self,
        of: K::Of,
        value: K::Value,
        as_of_ms: i64,
    ) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::WorkedOut { value, as_of_ms }).await
    }

    /// Keeps that something keeps the observation of `of` up to date from now on, such as the hub reading again what a
    /// client follows, so each read of it from now is current. What was read before isn't, so it stays as of when it was read until
    /// the next read.
    pub(crate) async fn observation_kept_current<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::KeptCurrent).await
    }

    /// Keeps that nothing watches the observation of `of` any more, since `as_of_ms`, by the hub's clock, so one that
    /// was current is as of then, which clients are told: what watched it, such as a worker watching a worktree, would
    /// have said of any change before. For what's kept up to date by reading it again, see
    /// [`Hub::observation_no_longer_read_again`].
    pub(crate) async fn observation_let_go<K: Kind>(&self, of: K::Of, as_of_ms: i64) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::LetGo { as_of_ms }).await
    }

    /// Keeps that the hub no longer reads the observation of `of` again to keep it up to date, as GitHub's are while a
    /// client follows them, so one that was current is as of its last read, which clients are told. Unlike
    /// [`Hub::observation_let_go`], nothing said of a change between the last read and now, so it's as of no later.
    pub(crate) async fn observation_no_longer_read_again<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::NoLongerReadAgain).await
    }

    /// What the observation of `of` says, however fresh it is, for what the hub decides from it. `None` where it isn't
    /// known, which is never taken for "none".
    pub(crate) fn observation_value<K: Kind>(&self, of: &K::Of) -> Option<K::Value> {
        self.observations.value_of::<K>(of)
    }

    /// Keeps that the hub keeps every observation of kind `K` up to date from now on, by reading each again on a
    /// schedule of its own, as it reads every GitHub account's username each day, so each read of any from now is
    /// current. What was read before isn't, so each stays as of when it was read until its next read.
    pub(crate) async fn observations_kept_current<K: Kind>(&self) {
        let _changing = self.observations.changing.lock().await;
        K::table(&mut self.observations.tables.lock()).all_kept_current = true;
    }

    /// Keeps that the hub no longer reads the observations of kind `K` again on its schedule, as
    /// [`Hub::observations_kept_current`] said it did, as when it stops, so each that was current is as of its last read,
    /// which clients are told, as [`Hub::observation_no_longer_read_again`] does for one.
    pub(crate) async fn observations_no_longer_read_again<K: Kind>(&self) {
        {
            let _changing = self.observations.changing.lock().await;
            K::table(&mut self.observations.tables.lock()).all_kept_current = false;
        }
        for of in self.observations.kept_of::<K>() {
            if let Err(e) = self.change_observation::<K>(of, |_| Change::NoLongerReadAgain).await {
                warn!("saying an observation is as of its last read: {}", e.message);
            }
        }
    }

    /// Whether something keeps the observation of `of` up to date now, as [`Hub::observation_kept_current`] said.
    pub(crate) fn observation_is_kept_current<K: Kind>(&self, of: &K::Of) -> bool {
        K::table(&mut self.observations.tables.lock()).kept_current(of)
    }

    /// Holds the lock every change to an observation and every list takes, until what this returns is dropped, so each
    /// waits as it would for a change under way.
    #[cfg(test)]
    pub(crate) async fn holding_observations(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.observations.changing.lock().await
    }

    /// Forgets the observation of `of`, since what it's of is gone, such as a workspace, and tells clients it's gone.
    pub(crate) async fn observation_gone<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::Gone).await
    }

    /// Forgets each observation of kind `K` that `gone` picks out, and tells clients, as [`Hub::observation_gone`] does
    /// of each, under the lock every change takes from before it picks them out. So a read kept while it was still
    /// wanted, as [`Hub::observation_read_while`] says, is forgotten, where the caller stopped wanting it first, and one
    /// that asks after finds it isn't wanted. Stops at the first that fails, which only a database that gives no
    /// revision has.
    pub(crate) async fn observations_gone_where<K: Kind>(&self, gone: impl Fn(&K::Of) -> bool) -> Result<(), ApiError> {
        let _changing = self.observations.changing.lock().await;
        for of in self.observations.kept_of::<K>().into_iter().filter(|of| gone(of)) {
            self.changed_observation_held::<K>(of, |_| Change::Gone).await?;
        }
        Ok(())
    }

    /// Forgets every observation of workspace `id`, and of its other worktrees and its sessions, of every kind, since the
    /// workspace is gone, and tells clients. One operation, so no kind is left out. Called where a workspace is
    /// discarded, and again by whatever may have kept one of it as it went, as a read that lands after can. A failure,
    /// which only a database that gives no revision has, is only logged, and
    /// [`Hub::observations_of_no_workspace_gone`] forgets what's left.
    pub(crate) async fn workspace_observations_gone(&self, id: &WorkspaceId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|owner| owner.workspace() == Some(id)).await {
            warn!(%id, "forgetting the observations of a workspace that's gone: {}", e.message);
        }
    }

    /// Forgets every observation of the GitHub account `id`, of every kind, since the account is gone, and tells clients,
    /// as [`Hub::workspace_observations_gone`] does for a workspace. Called where an account is removed, and again by
    /// whatever may have kept one of it as it went, as a read of its username that lands after can. What a failure
    /// leaves is forgotten as the hub next starts.
    pub(crate) async fn github_account_observations_gone(&self, id: &GithubAccountId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|owner| owner == Owner::GithubAccount(id)).await {
            warn!(%id, "forgetting the observations of a GitHub account that's gone: {}", e.message);
        }
    }

    /// Forgets every observation of the app of your own `id`, of every kind, since the app is gone, and tells clients, as
    /// [`Hub::github_account_observations_gone`] does for a GitHub account. What a failure leaves is forgotten as the
    /// hub next starts.
    pub(crate) async fn app_observations_gone(&self, id: GithubAppId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|owner| owner == Owner::App(&id)).await {
            warn!(%id, "forgetting the observations of an app of your own that's gone: {}", e.message);
        }
    }

    /// Forgets every observation of a GitHub account the hub doesn't have, and tells clients, as one tidying removed as
    /// the hub starts leaves. The accounts are listed under the lock every change takes, as
    /// [`Hub::observations_of_no_workspace_gone`] lists the workspaces.
    pub(crate) async fn observations_of_no_github_account_gone(&self) {
        let _changing = self.observations.changing.lock().await;
        let have = match self.blocking(Store::github_account_ids).await {
            Ok(have) => have,
            Err(e) => return warn!("listing GitHub accounts to forget the observations of those gone: {}", e.message),
        };
        let gone = |owner: Owner<'_>| matches!(owner, Owner::GithubAccount(id) if !have.contains(id));
        if let Err(e) = self.forget_observations_of(&gone).await {
            warn!("forgetting the observations of GitHub accounts that are gone: {}", e.message);
        }
    }

    /// Forgets every observation of a workspace the hub doesn't have, and of its sessions, and tells clients: what a
    /// hub that stopped between removing a workspace and forgetting its observations left, or a working-out kept after
    /// a removal it missed. The workspaces are listed under the lock every change takes, so none made since has one
    /// yet.
    pub(crate) async fn observations_of_no_workspace_gone(&self) {
        let _changing = self.observations.changing.lock().await;
        let workspaces = match self.workspace_list(true).await {
            Ok(workspaces) => workspaces,
            Err(e) => return warn!("listing workspaces to forget the observations of those gone: {}", e.message),
        };
        let have: HashSet<WorkspaceId> = workspaces.into_iter().map(|workspace| workspace.id).collect();
        let gone = |owner: Owner<'_>| owner.workspace().is_some_and(|id| !have.contains(id));
        if let Err(e) = self.forget_observations_of(&gone).await {
            warn!("forgetting the observations of workspaces that are gone: {}", e.message);
        }
    }

    /// Forgets every observation of session `key`, of every kind, since the session is gone, and tells clients, as
    /// [`Hub::workspace_observations_gone`] does a workspace's.
    pub(crate) async fn session_observations_gone(&self, key: &SessionKey) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|owner| owner == Owner::Session(key)).await {
            warn!(session = %key.session_id, "forgetting the observations of a session that's gone: {}", e.message);
        }
    }

    /// Forgets every observation of an owner that `gone` picks out, under one revision, since a client compares a
    /// removal's revision only with what it holds of that observation and with its level's list. The caller holds the
    /// lock every change takes. Changes nothing where there's no revision to give.
    async fn forget_observations_of(&self, gone: Gone<'_>) -> Result<(), ApiError> {
        let observations = &self.observations;
        if !observations.tables.lock().keep_of(gone) {
            return Ok(());
        }
        let rev = self.blocking(Store::next_rev).await?;
        for observation in observations.tables.lock().forget_of(gone, &observations.writer) {
            self.tell_of_observation(ObservationNotice::Removed { observation, rev });
        }
        Ok(())
    }

    /// Waits until every change made before has been written to the cache file, as the hub does as it stops, so the
    /// next hub starts with them.
    pub(crate) async fn observations_written(&self) {
        self.observations.writer.written().await;
    }

    /// Makes `change` to the observation of `of`, under the lock every change takes, from before it takes a revision
    /// until clients are told, so a list of records has every change its revision takes in. Its write to the file is
    /// queued under the lock too, so the file is written in the order the changes are made, and made after, so no list
    /// waits on the file. Fails only where the database gives no revision, and then changes nothing but whether
    /// something keeps it current.
    async fn change_observation<K: Kind>(
        &self,
        of: K::Of,
        change: impl FnOnce(&Table<K>) -> Change<K::Value>,
    ) -> Result<(), ApiError> {
        self.change_observation_while::<K>(of, || true, change).await
    }

    /// What [`Hub::change_observation`] does, if `wanted` says the change is still wanted, as
    /// [`Hub::observation_read_while`] says.
    async fn change_observation_while<K: Kind>(
        &self,
        of: K::Of,
        wanted: impl FnOnce() -> bool,
        change: impl FnOnce(&Table<K>) -> Change<K::Value>,
    ) -> Result<(), ApiError> {
        let _changing = self.observations.changing.lock().await;
        if !wanted() {
            return Ok(());
        }
        self.changed_observation_held::<K>(of, change).await
    }

    /// What [`Hub::change_observation_while`] does once it holds the lock every change takes, which the caller holds.
    async fn changed_observation_held<K: Kind>(
        &self,
        of: K::Of,
        change: impl FnOnce(&Table<K>) -> Change<K::Value>,
    ) -> Result<(), ApiError> {
        let observations = &self.observations;
        let step = {
            let mut tables = observations.tables.lock();
            let table = K::table(&mut tables);
            let change = change(table);
            table.step(&of, change)
        };
        let changed = match step {
            Step::Same => return Ok(()),
            Step::Reread(said) => {
                observations.writer.write::<K>(of.clone(), Write::ReadAgain { read_at_ms: said.read_at_ms });
                K::table(&mut observations.tables.lock()).reread(&of, said);
                return Ok(());
            }
            Step::Reheard(said) => {
                let write = Write::Says { value: said.value.clone(), read_at_ms: said.read_at_ms };
                observations.writer.write::<K>(of.clone(), write);
                K::table(&mut observations.tables.lock()).reread(&of, said);
                return Ok(());
            }
            Step::Changed(changed) => changed,
        };
        let rev = self.blocking(Store::next_rev).await?;
        let write = match &changed {
            Changed::Says(said) => Write::Says { value: said.value.clone(), read_at_ms: said.read_at_ms },
            Changed::Gone => Write::Gone,
        };
        observations.writer.write::<K>(of.clone(), write);
        let notice = K::table(&mut observations.tables.lock()).change(&of, changed, rev);
        if let Some(notice) = notice {
            self.tell_of_observation(notice);
        }
        Ok(())
    }

    /// Tells clients of a change to an observation, the one place the hub sends an observation's notice. Which clients
    /// are sent it is decided where every notice's is, by its level, in `crate::server::subscribe`.
    fn tell_of_observation(&self, notice: ObservationNotice) {
        self.notify(notice.into());
    }
}
