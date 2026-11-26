//! What a workspace's worker says, which the store keeps only while it's still the workspace's worker, as
//! `docs/principles/keeping-observations.md` says, so a worker that's been replaced, and may still have frames on the
//! way, never has the last word.

use std::sync::Arc;

use protocol::model::WorkspaceId;

use super::failing::Keeping;
use super::keepers::Keeper;
use super::kept::Change;
use super::kinds::Kind;
use crate::hub::Hub;
use crate::store::now_ms;
use crate::worker_link::WorkerLink;

/// The one way the hub keeps what a workspace's worker says, or keeps current. It's had only for the workspace's worker
/// at the time, and each change it makes asks again whether it still is, under the lock every change takes, so one
/// replaced meanwhile changes nothing, and nothing it says lands after [`Hub::keeper_went`] let go of it.
pub(crate) struct WorkerWord<'a> {
    hub: &'a Hub,
    workspace_id: &'a WorkspaceId,
    link: &'a Arc<WorkerLink>,
}

impl Hub {
    /// The word of `link`, if it's the workspace `workspace_id`'s worker now.
    pub(crate) fn worker_word<'a>(
        &'a self,
        workspace_id: &'a WorkspaceId,
        link: &'a Arc<WorkerLink>,
    ) -> Option<WorkerWord<'a>> {
        let word = WorkerWord { hub: self, workspace_id, link };
        word.still_its().then_some(word)
    }
}

impl WorkerWord<'_> {
    /// The worker.
    pub(crate) fn link(&self) -> &Arc<WorkerLink> {
        self.link
    }

    /// Keeps that the worker read `value` of `of` at `read_at_ms`, as [`Hub::observation_read`] does, current if it
    /// keeps it current, while it's still the workspace's worker.
    pub(crate) async fn read<K: Kind>(&self, of: K::Of, value: K::Value, read_at_ms: i64) -> Keeping {
        self.hub.observation_read_while::<K>(of, value, read_at_ms, || self.still_its()).await
    }

    /// Keeps that the worker keeps the observation of `of` current from now, as it watches it, while it's still the
    /// workspace's worker, as [`Hub::observation_kept_by`] does.
    pub(crate) async fn keeps<K: Kind>(&self, of: K::Of) -> Keeping {
        let keeper = self.keeper();
        let change = |_: &_| Change::KeptBy { keeper, at_ms: now_ms() };
        self.hub.change_observation_while::<K>(of, || self.still_its(), change).await
    }

    /// Keeps that the worker said `value` of `of`, read at `read_at_ms`, and keeps it current from now, while it's
    /// still the workspace's worker, as [`Hub::observation_said_by_while`] does.
    pub(crate) async fn said<K: Kind>(&self, of: K::Of, value: K::Value, read_at_ms: i64) -> Keeping {
        let keeper = self.keeper();
        self.hub.observation_said_by_while::<K>(of, keeper, value, read_at_ms, || self.still_its()).await
    }

    /// What the store keeps it as, as what keeps an observation.
    fn keeper(&self) -> Keeper {
        Keeper::Worker(self.link.id)
    }

    /// Whether it's still the workspace's worker.
    fn still_its(&self) -> bool {
        self.link.is_current(&self.hub.workers.lock(), self.workspace_id)
    }
}
