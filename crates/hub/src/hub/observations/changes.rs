//! How the code that reads something changes what the store keeps of it, and the one place that tells clients.

use protocol::client::{ApiError, ObservationNotice};

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

    /// Tells clients of a change to an observation. The one place the hub sends an observation's notice, so what decides
    /// which clients are sent it decides it here.
    fn tell_of_observation(&self, notice: ObservationNotice) {
        self.notify(notice.into());
    }
}
