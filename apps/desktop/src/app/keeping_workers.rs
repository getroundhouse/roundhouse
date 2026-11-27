//! Telling the updater which workers its update has to keep running, and which release the hub's database is from,
//! from the hub's status, so the update crosses one break in the worker protocol at a time, as
//! `docs/principles/versions.md` says, and its hub can migrate the database. While the app is connected, the
//! oldest can only rise, as each workspace on a worker of another build moves to one of the hub's own, so the app reads
//! the status again whenever fewer workspaces are on one, and once an hour while the hub runs older workers, for one
//! that leaves without its workspace's record changing, such as one the hub is ending as it starts. *Unconfirmed:* when
//! it reads the status again.

use protocol::client::admin;
use std::time::Duration;

use client_core::records::{Snapshot, WorkerBuild};
use gpui_kit::{Context, Task};
use protocol::client::HubStatus;
use protocol::model::Workspace;

use super::{AppView, hub_source};

/// How long the app waits to read the status again while the hub runs workers older than its own.
const READ_AGAIN_AFTER: Duration = Duration::from_hours(1);

/// What the app keeps to read the status again: how many workspaces were on a worker of another build, and the read
/// it's waiting to make, if any.
#[derive(Default)]
pub(super) struct KeepingWorkers {
    moving: WorkersMoving,
    reading: Option<Task<()>>,
}

impl AppView {
    /// From a status the app has read of this computer's hub, as it connected or since. A paired hub says none as the
    /// app connects, since its workers are another computer's, which this app's update doesn't restart.
    pub(super) fn keep_workers_of(&mut self, status: &HubStatus, cx: &mut Context<Self>) {
        self.keeping.moving.seen(on_other_builds(&self.records));
        let heard =
            crate::updates::Heard { workers: hub_source::workers_to_keep(status), hub: status.hub_version.clone() };
        crate::updates::keep(heard, cx);
        self.keeping.reading = hub_source::runs_older_workers(status).then(|| read_status_after(READ_AGAIN_AFTER, cx));
    }

    /// Once the records say fewer workspaces are on a worker of another build, reads the status again. Only for this
    /// computer's hub, since a paired hub's workers are another computer's, which this app's update doesn't restart.
    pub(super) fn keep_workers_as_they_move(&mut self, cx: &mut Context<Self>) {
        if self.hub.remote.is_none() && self.keeping.moving.moved(on_other_builds(&self.records)) {
            self.keeping.reading = Some(read_status_after(Duration::ZERO, cx));
        }
    }
}

/// Reads the status once `wait` is up, from the hub the app is connected to then, and keeps its workers.
fn read_status_after(wait: Duration, cx: &mut Context<AppView>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        cx.background_executor().timer(wait).await;
        let asked = this.update(cx, |this, _| this.is_connected().then(|| this.linked.ask(admin::HubStatus)));
        let Ok(Some(reading)) = asked else { return };
        // A hub that doesn't answer is going, and the app reads the next one's status as it connects to it.
        let Ok(status) = reading.recv().await else { return };
        let _ = this.update(cx, |this, cx| this.keep_workers_of(&status, cx));
    })
}

/// How many workspaces the records last said are on a worker of another build, so the app can tell when one has moved.
#[derive(Default)]
struct WorkersMoving(usize);

impl WorkersMoving {
    /// As the records say when the app reads the status.
    fn seen(&mut self, on_other_builds: usize) {
        self.0 = on_other_builds;
    }

    /// Whether fewer are on one than the records said last, which is when the oldest worker the hub runs can rise.
    fn moved(&mut self, on_other_builds: usize) -> bool {
        let moved = on_other_builds < self.0;
        self.0 = on_other_builds;
        moved
    }
}

fn on_other_builds(records: &Snapshot) -> usize {
    let other = |workspace: &&Workspace| matches!(records.worker_build(workspace), WorkerBuild::Other(_));
    records.workspaces().iter().filter(other).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fewer_workspaces_on_another_build_than_last_said_is_a_move() {
        let mut moving = WorkersMoving::default();
        moving.seen(3);

        assert!(!moving.moved(3), "the same");
        assert!(!moving.moved(4), "one more, such as a workspace the hub carried on with");
        assert!(moving.moved(2));
        assert!(!moving.moved(2), "counted once");
        moving.seen(5);
        assert!(moving.moved(4), "from what the status read last");
    }
}
