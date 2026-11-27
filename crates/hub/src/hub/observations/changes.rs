//! How the code that reads something changes what the store keeps of it, and the one place that tells clients.

use std::collections::HashSet;

use protocol::client::{ApiError, ObservationNotice};
use protocol::model::{GithubAccountId, GithubAppId, WorkspaceId};
use tracing::{debug, warn};

use super::failing::{self, Keeping};
use super::file::Write;
use super::kept::{Change, Changed, Said, Step, Table};
use super::kinds::{GoesWith, Kind, SessionKey};
use super::tables::Gone;
use crate::hub::Hub;
use crate::store::{Store, now_ms};

impl Hub {
    /// Keeps that the observation of `of` was read at `read_at_ms`, by the hub's clock, and found `value`. It's current
    /// if something keeps it up to date, as [`Hub::observation_kept_by`] says, and otherwise as of `read_at_ms`. A
    /// read that finds what's kept tells clients nothing and moves no revision, and the file keeps only that it was
    /// read then, so a hub that starts again has it as of this read. One that began before the read kept, and lands
    /// after it, is dropped, and so is one of what goes with a record that isn't there any more, as
    /// [`Hub::change_observation`] says.
    pub(crate) async fn observation_read<K: Kind>(&self, of: K::Of, value: K::Value, read_at_ms: i64) -> Keeping {
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
    ) -> Keeping {
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
    ) -> Keeping {
        let held = of.clone();
        self.change_observation::<K>(of, |table| Change::Merged { value: merge(table.value(&held)), heard_at_ms }).await
    }

    /// Keeps that the observation of `of`, worked out now from what it's worked out from, says `value`, as of
    /// `as_of_ms`, when what it's worked out from was read. It's current if something keeps it up to date, as
    /// [`Hub::observation_kept_by`] says, and otherwise as of `as_of_ms`. Unlike [`Hub::observation_read`], it
    /// drops nothing as a late read, since a value worked out from what's kept now is never older than one worked out
    /// before, even where what it's worked out from was read earlier, as when a workspace's pull request becomes
    /// another. So whatever works out a kind works each out in turn.
    pub(crate) async fn observation_worked_out<K: Kind>(&self, of: K::Of, value: K::Value, as_of_ms: i64) -> Keeping {
        self.change_observation::<K>(of, |_| Change::WorkedOut { value, as_of_ms }).await
    }

    /// Keeps the observation of `of` as `again` leaves what it says, as of when that was read, for a part of what's read
    /// that the hub works out from other state, as that changes. Under the lock every change takes, so a read that lands
    /// meanwhile isn't undone. Where nothing is kept, there's nothing to work out again.
    pub(crate) async fn observation_worked_out_again<K: Kind>(
        &self,
        of: K::Of,
        again: impl FnOnce(&mut K::Value),
    ) -> Keeping {
        let _changing = self.observations.changing.lock().await;
        let kept = K::table(&mut self.observations.tables.lock()).said(&of).cloned();
        let Some(Said { mut value, read_at_ms, .. }) = kept else { return Ok(()).into() };
        again(&mut value);
        self.changed_observation_held::<K>(of, |_| Change::WorkedOut { value, as_of_ms: read_at_ms }).await.into()
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
    /// [`Hub::observations_kept_current`] said it did, as when it stops, so each that was current is as its kind says
    /// once unkept, which clients are told, as [`Hub::observation_no_longer_kept_by`] does for one.
    pub(crate) async fn observations_no_longer_kept<K: Kind>(&self) {
        {
            let _changing = self.observations.changing.lock().await;
            K::table(&mut self.observations.tables.lock()).all_kept_current = false;
        }
        for of in self.observations.kept_of::<K>() {
            let at_ms = now_ms();
            self.change_observation::<K>(of, |_| Change::NoLongerKeptWithItsKind { at_ms }).await;
        }
    }

    /// Holds the lock every change to an observation and every list takes, until what this returns is dropped, so each
    /// waits as it would for a change under way.
    #[cfg(test)]
    pub(crate) async fn holding_observations(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.observations.changing.lock().await
    }

    /// Forgets the observation of `of`, since what it's of is gone, such as a workspace, and tells clients it's gone.
    pub(crate) async fn observation_gone<K: Kind>(&self, of: K::Of) -> Keeping {
        self.change_observation::<K>(of, |_| Change::Gone).await
    }

    /// Forgets each observation of kind `K` that `gone` picks out, and tells clients, as [`Hub::observation_gone`] does
    /// of each, under the lock every change takes from before it picks them out. So a read kept while it was still
    /// wanted, as [`Hub::observation_read_while`] says, is forgotten, where the caller stopped wanting it first, and one
    /// that asks after finds it isn't wanted. Stops at the first that fails, which only a database that gives no
    /// revision has.
    pub(crate) async fn observations_gone_where<K: Kind>(&self, gone: impl Fn(&K::Of) -> bool) -> Keeping {
        let _changing = self.observations.changing.lock().await;
        for of in self.observations.kept_of::<K>().into_iter().filter(|of| gone(of)) {
            if let Err(e) = self.changed_observation_held::<K>(of, |_| Change::Gone).await {
                return Err(e).into();
            }
        }
        Ok(()).into()
    }

    /// Forgets every observation of workspace `id`, and of its worktrees and its sessions, of every kind, since the
    /// workspace is gone, and tells clients. One operation, so no kind is left out. Called where a workspace is
    /// discarded, once its record is, so a read that lands after is never kept, as [`Hub::observation_read`] says. A
    /// failure, which only a database that gives no revision has, is only logged, and
    /// [`Hub::observations_of_no_workspace_gone`] forgets what's left.
    pub(crate) async fn workspace_observations_gone(&self, id: &WorkspaceId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|goes_with| goes_with.workspace() == Some(id)).await {
            warn!(
                name: "observation.workspace_unforgotten",
                workspace_id = %id,
                error = e.message,
                "Couldn't forget the observations of a workspace that's gone"
            );
        }
    }

    /// Forgets every observation of workspace `id` that goes with its own worktree, of every kind, such as its lines,
    /// its changed files and its branches, since the workspace is archived, and tells clients, as
    /// [`Hub::workspace_observations_gone`] does for all of a workspace's. Called once its record says it's archived,
    /// so a read that lands after is never kept, and what its worker says once it's restored is. What a failure
    /// leaves is forgotten as the hub next starts.
    pub(crate) async fn worktree_observations_gone(&self, id: &WorkspaceId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|goes_with| goes_with == GoesWith::Worktree(id)).await {
            warn!(
                name: "observation.worktree_unforgotten",
                workspace_id = %id,
                error = e.message,
                "Couldn't forget the observations of an archived workspace's worktree"
            );
        }
    }

    /// Forgets every observation of the GitHub account `id`, of every kind, since the account is gone, and tells clients,
    /// as [`Hub::workspace_observations_gone`] does for a workspace. Called where an account is removed, once its record
    /// is, so a read of its username that lands after is never kept. What a failure leaves is forgotten as the hub next
    /// starts.
    pub(crate) async fn github_account_observations_gone(&self, id: &GithubAccountId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|goes_with| goes_with == GoesWith::GithubAccount(id)).await {
            warn!(
                name: "observation.account_unforgotten",
                account = %id,
                error = e.message,
                "Couldn't forget the observations of a GitHub account that's gone"
            );
        }
    }

    /// Forgets every observation of the app of your own `id`, of every kind, since the app is gone, and tells clients, as
    /// [`Hub::github_account_observations_gone`] does for a GitHub account. What a failure leaves is forgotten as the
    /// hub next starts.
    pub(crate) async fn app_observations_gone(&self, id: GithubAppId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|goes_with| goes_with == GoesWith::App(&id)).await {
            warn!(
                name: "observation.app_unforgotten",
                app = %id,
                error = e.message,
                "Couldn't forget the observations of an app of your own that's gone"
            );
        }
    }

    /// Forgets every observation of a GitHub account the hub doesn't have, and tells clients, as one tidying removed as
    /// the hub starts leaves. The accounts are listed under the lock every change takes, as
    /// [`Hub::observations_of_no_workspace_gone`] lists the workspaces.
    pub(crate) async fn observations_of_no_github_account_gone(&self) {
        let _changing = self.observations.changing.lock().await;
        let have = match self.blocking(Store::github_account_ids).await {
            Ok(have) => have,
            Err(e) => {
                return warn!(
                    name: "observation.accounts_unlisted",
                    error = e.message,
                    "Couldn't list GitHub accounts to forget the observations of those gone"
                );
            }
        };
        let gone = |goes_with: GoesWith<'_>| matches!(goes_with, GoesWith::GithubAccount(id) if !have.contains(id));
        if let Err(e) = self.forget_observations_of(&gone).await {
            warn!(
                name: "observation.accounts_unforgotten",
                error = e.message,
                "Couldn't forget the observations of GitHub accounts that are gone"
            );
        }
    }

    /// Forgets every observation of a workspace the hub doesn't have, and of its sessions, and tells clients: what a
    /// hub that stopped between removing a workspace and forgetting its observations left, where the store couldn't
    /// read which records are there as it opened. The workspaces are listed under the lock every change takes, so none
    /// made since has one yet.
    pub(crate) async fn observations_of_no_workspace_gone(&self) {
        let _changing = self.observations.changing.lock().await;
        let workspaces = match self.workspace_list(true).await {
            Ok(workspaces) => workspaces,
            Err(e) => {
                return warn!(
                    name: "observation.workspaces_unlisted",
                    error = e.message,
                    "Couldn't list workspaces to forget the observations of those gone"
                );
            }
        };
        let have: HashSet<WorkspaceId> = workspaces.into_iter().map(|workspace| workspace.id).collect();
        let gone = |goes_with: GoesWith<'_>| goes_with.workspace().is_some_and(|id| !have.contains(id));
        if let Err(e) = self.forget_observations_of(&gone).await {
            warn!(
                name: "observation.workspaces_unforgotten",
                error = e.message,
                "Couldn't forget the observations of workspaces that are gone"
            );
        }
    }

    /// Forgets every observation of session `key`, of every kind, since the session is gone, and tells clients, as
    /// [`Hub::workspace_observations_gone`] does a workspace's.
    pub(crate) async fn session_observations_gone(&self, key: &SessionKey) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|goes_with| goes_with == GoesWith::Session(key)).await {
            warn!(
                name: "observation.session_unforgotten",
                session_id = %key.session_id,
                error = e.message,
                "Couldn't forget the observations of a session that's gone"
            );
        }
    }

    /// Forgets every observation that goes with what `gone` picks out, under one revision, since a client compares a
    /// removal's revision only with what it holds of that observation and with its level's list, and what keeps each,
    /// including one that has said nothing yet. The caller holds the lock every change takes. Changes nothing it would
    /// tell of where there's no revision to give.
    async fn forget_observations_of(&self, gone: Gone<'_>) -> Result<(), ApiError> {
        let observations = &self.observations;
        if !observations.tables.lock().keep_of(gone) {
            // Nothing said to forget, nor tell of, but what keeps any goes with it all the same.
            observations.tables.lock().forget_of(gone, &observations.writer);
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
    /// waits on the file. A change to what it says keeps nothing where the record it goes with isn't there, as
    /// [`Kind::goes_with`] says, which the database says as it gives the revision, so a word that lands after a
    /// workspace was removed, or its worktree archived, is never kept, whoever said it. Fails only where the database
    /// gives no revision, which is logged, and then changes nothing but whether something keeps it current.
    pub(super) async fn change_observation<K: Kind>(
        &self,
        of: K::Of,
        change: impl FnOnce(&Table<K>) -> Change<K::Value>,
    ) -> Keeping {
        self.change_observation_while::<K>(of, || true, change).await
    }

    /// What [`Hub::change_observation`] does, if `wanted` says the change is still wanted, as
    /// [`Hub::observation_read_while`] says.
    pub(super) async fn change_observation_while<K: Kind>(
        &self,
        of: K::Of,
        wanted: impl FnOnce() -> bool,
        change: impl FnOnce(&Table<K>) -> Change<K::Value>,
    ) -> Keeping {
        let _changing = self.observations.changing.lock().await;
        if !wanted() {
            return Ok(()).into();
        }
        self.changed_observation_held::<K>(of, change).await.into()
    }

    /// What [`Hub::change_observation_while`] does once it holds the lock every change takes, which the caller holds.
    pub(super) async fn changed_observation_held<K: Kind>(
        &self,
        of: K::Of,
        change: impl FnOnce(&Table<K>) -> Change<K::Value>,
    ) -> Result<(), ApiError> {
        let observations = &self.observations;
        let (step, doing) = {
            let mut tables = observations.tables.lock();
            let table = K::table(&mut tables);
            let change = change(table);
            let doing = change.doing();
            (table.step(&of, change), doing)
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
        // What it says is kept only while the record it goes with is there, which the database says as it gives the
        // revision. Forgetting it needs no record.
        let there = match &changed {
            Changed::Says(_) => K::goes_with(&of).and_then(GoesWith::record),
            Changed::Gone => None,
        };
        let rev = self.blocking(move |store| store.next_rev_while_there(there.as_ref())).await;
        let Some(rev) = rev.inspect_err(|e| failing::logged::<K>(&of, doing, e))? else {
            // A word about what's gone, as a read that lands after its workspace was removed or archived, of which
            // nothing is kept, not even what would keep it current.
            K::table(&mut observations.tables.lock()).keepers.remove(&of);
            debug!(name: "observation.of_nothing_there", table = K::FORM.table, "Kept nothing of a record that's gone");
            return Ok(());
        };
        let notice = K::table(&mut observations.tables.lock()).change(&of, changed, rev, &observations.writer);
        if let Some(notice) = notice {
            self.tell_of_observation(notice);
        }
        Ok(())
    }

    /// Tells clients of a change to an observation, the one place the hub sends an observation's notice. Which clients
    /// are sent it is decided where every notice's is, by its level, in `crate::server::subscribe`.
    pub(super) fn tell_of_observation(&self, notice: ObservationNotice) {
        self.notify(notice.into());
    }
}
