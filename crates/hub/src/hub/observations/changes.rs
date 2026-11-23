//! How the code that reads something changes what the store keeps of it, and the one place that tells clients.

use std::collections::HashSet;

use protocol::client::{ApiError, ObservationNotice};
use protocol::model::WorkspaceId;
use tracing::warn;

use super::file::Write;
use super::kept::{Change, Changed, Step};
use super::kinds::Kind;
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
        self.change_observation::<K>(of, Change::Read { value, read_at_ms }).await
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
        self.change_observation::<K>(of, Change::WorkedOut { value, as_of_ms }).await
    }

    /// Keeps that something keeps the observation of `of` up to date from now on, such as the hub reading again what a
    /// client follows, so each read of it from now is current. What was read before isn't, so it stays as of when it was read until
    /// the next read.
    pub(crate) async fn observation_kept_current<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, Change::KeptCurrent).await
    }

    /// Keeps that nothing watches the observation of `of` any more, since `as_of_ms`, by the hub's clock, so one that
    /// was current is as of then, which clients are told: what watched it, such as a worker watching a worktree, would
    /// have said of any change before. For what's kept up to date by reading it again, see
    /// [`Hub::observation_no_longer_read_again`].
    pub(crate) async fn observation_let_go<K: Kind>(&self, of: K::Of, as_of_ms: i64) -> Result<(), ApiError> {
        self.change_observation::<K>(of, Change::LetGo { as_of_ms }).await
    }

    /// Keeps that the hub no longer reads the observation of `of` again to keep it up to date, as GitHub's are while a
    /// client follows them, so one that was current is as of its last read, which clients are told. Unlike
    /// [`Hub::observation_let_go`], nothing said of a change between the last read and now, so it's as of no later.
    pub(crate) async fn observation_no_longer_read_again<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, Change::NoLongerReadAgain).await
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
        self.change_observation::<K>(of, Change::Gone).await
    }

    /// Forgets every observation of workspace `id`, of every kind, since the workspace is gone, and tells clients. One
    /// operation, so no kind is left out. Called where a workspace is discarded, and again by whatever may have kept
    /// one of it as it went, as a read that lands after can. A failure, which only a database that gives no revision
    /// has, is only logged, and [`Hub::observations_of_no_workspace_gone`] forgets what's left.
    pub(crate) async fn workspace_observations_gone(&self, id: &WorkspaceId) {
        let _changing = self.observations.changing.lock().await;
        if let Err(e) = self.forget_observations_of(&|of| of == id).await {
            warn!(%id, "forgetting the observations of a workspace that's gone: {}", e.message);
        }
    }

    /// Forgets every observation of a workspace the hub doesn't have, and tells clients: what a hub that stopped
    /// between removing a workspace and forgetting its observations left, or a working-out kept after a removal it
    /// missed. The workspaces are listed under the lock every change takes, so none made since has one yet.
    pub(crate) async fn observations_of_no_workspace_gone(&self) {
        let _changing = self.observations.changing.lock().await;
        let workspaces = match self.workspace_list(true).await {
            Ok(workspaces) => workspaces,
            Err(e) => return warn!("listing workspaces to forget the observations of those gone: {}", e.message),
        };
        let have: HashSet<WorkspaceId> = workspaces.into_iter().map(|workspace| workspace.id).collect();
        if let Err(e) = self.forget_observations_of(&|id| !have.contains(id)).await {
            warn!("forgetting the observations of workspaces that are gone: {}", e.message);
        }
    }

    /// Forgets every observation of a workspace that `gone` picks out, under one revision, since a client compares a
    /// removal's revision only with what it holds of that observation and with its level's list. The caller holds the
    /// lock every change takes. Changes nothing where there's no revision to give.
    async fn forget_observations_of(&self, gone: &(dyn Fn(&WorkspaceId) -> bool + Sync)) -> Result<(), ApiError> {
        let observations = &self.observations;
        if !observations.tables.lock().keep_of(gone) {
            return Ok(());
        }
        let rev = self.blocking(Store::next_rev).await?;
        let forgotten = observations.tables.lock().forget_of(gone, &observations.writer);
        for observation in forgotten {
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
    async fn change_observation<K: Kind>(&self, of: K::Of, change: Change<K::Value>) -> Result<(), ApiError> {
        let observations = &self.observations;
        let _changing = observations.changing.lock().await;
        let step = K::table(&mut observations.tables.lock()).step(&of, change);
        let changed = match step {
            Step::Same => return Ok(()),
            Step::Reread(said) => {
                observations.writer.write::<K>(of.clone(), Write::ReadAgain { read_at_ms: said.read_at_ms });
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
        self.tell_of_observation(notice);
        Ok(())
    }

    /// Tells clients of a change to an observation, the one place the hub sends an observation's notice. Which clients
    /// are sent it is decided where every notice's is, in `crate::server::sends`.
    fn tell_of_observation(&self, notice: ObservationNotice) {
        self.notify(notice.into());
    }
}
