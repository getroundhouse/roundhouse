//! How the code that reads something changes what the store keeps of it, and the one place that tells clients.

use protocol::client::{ApiError, ObservationNotice};

use super::file::{self, Write};
use super::kept::{Change, Changed, Step};
use super::kinds::Kind;
use crate::hub::Hub;

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

    /// Keeps that something keeps the observation of `of` up to date from now on, such as a worker watching a worktree,
    /// so each read of it from now is current. What was read before isn't, so it stays as of when it was read until
    /// the next read.
    pub(crate) async fn observation_kept_current<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, Change::KeptCurrent).await
    }

    /// Keeps that nothing keeps the observation of `of` up to date any more, since `as_of_ms`, by the hub's clock, so
    /// one that was current is as of then, which clients are told.
    pub(crate) async fn observation_let_go<K: Kind>(&self, of: K::Of, as_of_ms: i64) -> Result<(), ApiError> {
        self.change_observation::<K>(of, Change::LetGo { as_of_ms }).await
    }

    /// Whether something keeps the observation of `of` up to date now, as [`Hub::observation_kept_current`] said.
    pub(crate) fn observation_is_kept_current<K: Kind>(&self, of: &K::Of) -> bool {
        K::table(&mut self.observations.tables.lock()).kept_current(of)
    }

    /// Forgets the observation of `of`, since what it's of is gone, such as a workspace, and tells clients it's gone.
    pub(crate) async fn observation_gone<K: Kind>(&self, of: K::Of) -> Result<(), ApiError> {
        self.change_observation::<K>(of, Change::Gone).await
    }

    /// Makes `change` to the observation of `of`, under the lock every change takes, from before it takes a revision
    /// until clients are told, so a list of records has every change its revision takes in, and the file is written in
    /// the order the changes are made. Fails only where the database gives no revision, and then changes nothing but
    /// whether something keeps it current.
    async fn change_observation<K: Kind>(&self, of: K::Of, change: Change<K::Value>) -> Result<(), ApiError> {
        let observations = &self.observations;
        let _changing = observations.changing.lock().await;
        let step = K::table(&mut observations.tables.lock()).step(&of, change);
        let (takes_rev, write) = match &step {
            Step::Same => return Ok(()),
            Step::Reread(said) => (false, Write::ReadAgain { read_at_ms: said.read_at_ms }),
            Step::Changed(Changed::Says(said)) => {
                (true, Write::Says { value: said.value.clone(), read_at_ms: said.read_at_ms })
            }
            Step::Changed(Changed::Gone) => (true, Write::Gone),
        };
        let (cache, written) = (observations.file.clone(), of.clone());
        let rev = self
            .blocking(move |store| {
                let rev = takes_rev.then(|| store.next_rev()).transpose()?;
                file::write::<K>(&cache, &written, write);
                Ok(rev)
            })
            .await?;
        let notice = {
            let mut tables = observations.tables.lock();
            let table = K::table(&mut tables);
            match (step, rev) {
                (Step::Changed(changed), Some(rev)) => table.change(&of, changed, rev),
                (Step::Reread(said), _) => {
                    table.reread(&of, said);
                    return Ok(());
                }
                (Step::Same | Step::Changed(_), _) => return Ok(()),
            }
        };
        self.tell_of_observation(notice);
        Ok(())
    }

    /// Tells clients of a change to an observation. The one place the hub sends an observation's notice, so what decides
    /// which clients are sent it decides it here.
    fn tell_of_observation(&self, notice: ObservationNotice) {
        self.notify(notice.into());
    }
}
