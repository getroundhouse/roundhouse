//! How what keeps an observation, such as a workspace's worker or an agent's run, says so, and how all it keeps is let go
//! of with one call as it goes, so no kind it kept is left out.

use protocol::client::ApiError;
use tracing::warn;

use super::keepers::Keeper;
use super::kept::Change;
use super::kinds::Kind;
use crate::hub::Hub;
use crate::store::{Store, now_ms};

impl Hub {
    /// Keeps that `keeper` keeps the observation of `of` from now on, such as a worker watching its worktree, so each
    /// read of it from now is current if [`Keeper::keeps_current`] says so. What was read before isn't, so it stays as
    /// of when it was read until the next read. Another that kept it stops, so it's as its kind says once unkept, as
    /// when a run starts in place of one whose tool servers were only ever its own.
    pub(crate) async fn observation_kept_by<K: Kind>(&self, of: K::Of, keeper: Keeper) -> Result<(), ApiError> {
        self.change_observation::<K>(of, |_| Change::KeptBy { keeper, at_ms: now_ms() }).await
    }

    /// Keeps that `keeper` said `value` of `of`, read at `read_at_ms`, by the hub's clock, and keeps it from now on, as
    /// [`Hub::observation_kept_by`] and [`Hub::observation_read`] do, if `wanted` says it's still wanted, which it's
    /// asked under the lock every change takes. So what `keeper` says as it goes, such as a worker that's no longer the
    /// workspace's, is either kept before [`Hub::keeper_went`] lets go of it, or never kept.
    pub(crate) async fn observation_said_by_while<K: Kind>(
        &self,
        of: K::Of,
        keeper: Keeper,
        value: K::Value,
        read_at_ms: i64,
        wanted: impl FnOnce() -> bool,
    ) -> Result<(), ApiError> {
        let _changing = self.observations.changing.lock().await;
        if !wanted() {
            return Ok(());
        }
        self.changed_observation_held::<K>(of.clone(), |_| Change::KeptBy { keeper, at_ms: now_ms() }).await?;
        self.changed_observation_held::<K>(of, |_| Change::Read { value, read_at_ms }).await
    }

    /// Whether something keeps the observation of `of` up to date now, as [`Hub::observation_kept_by`] said.
    pub(crate) fn observation_is_kept_current<K: Kind>(&self, of: &K::Of) -> bool {
        K::table(&mut self.observations.tables.lock()).kept_current(of)
    }

    /// Lets go of every observation `keeper` keeps, of every kind, since it's gone, as a worker does once its stream
    /// ends: each is as its kind says once unkept, as of now, and clients are told. One operation under one revision, as
    /// [`Hub::workspace_observations_gone`] is, so no kind is left out. A failure, which only a database that gives no
    /// revision has, is only logged, and leaves each saying what it said, though nothing keeps it.
    pub(crate) async fn keeper_went(&self, keeper: Keeper) {
        let _changing = self.observations.changing.lock().await;
        let (observations, at_ms) = (&self.observations, now_ms());
        let changes = observations.tables.lock().change_as_it_goes(keeper, at_ms);
        let rev = if changes { self.blocking(Store::next_rev).await.map(Some) } else { Ok(None) };
        let rev = rev.unwrap_or_else(|e| {
            warn!(
                name: "observation.keeper_unsaid",
                ?keeper,
                error = e.message,
                "Couldn't say what's of what kept observations once it went"
            );
            None
        });
        for notice in observations.tables.lock().keeper_went(keeper, at_ms, rev, &observations.writer) {
            self.tell_of_observation(notice);
        }
    }

    /// Forgets every observation that nothing keeps of a kind that's gone once unkept, and tells clients, as the hub
    /// starts, once it has carried on with what an earlier hub left: what the cache file kept of each was only true of
    /// what kept it, such as the tool servers of a run it didn't carry on with, which is gone.
    pub(crate) async fn unkept_observations_gone(&self) {
        let _changing = self.observations.changing.lock().await;
        let observations = &self.observations;
        if !observations.tables.lock().keep_unkept() {
            return;
        }
        let rev = match self.blocking(Store::next_rev).await {
            Ok(rev) => rev,
            Err(e) => {
                return warn!(
                    name: "observation.unkept_unforgotten",
                    error = e.message,
                    "Couldn't forget what nothing keeps of what's only true while it's kept"
                );
            }
        };
        for notice in observations.tables.lock().forget_unkept(rev, &observations.writer) {
            self.tell_of_observation(notice);
        }
    }
}
