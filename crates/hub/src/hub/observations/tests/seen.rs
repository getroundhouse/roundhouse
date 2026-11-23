//! What the store's tests read observations as, and what they're told of them.

use protocol::client::HubNotice;
use protocol::model::{LineCounts, Observation, Observed, Rev};
use tokio::sync::broadcast;

use crate::hub::Hub;

/// Lines added and removed.
pub(super) fn lines(added: u64, removed: u64) -> LineCounts {
    LineCounts { added, removed }
}

/// The observation of `w`'s lines, saying `said`, at `rev`.
pub(super) fn of_w(said: Observed<LineCounts>, rev: Rev) -> Observation {
    Observation::WorkspaceLines { workspace_id: "w".into(), lines: said, rev }
}

/// `value`, as read at `as_of_ms`.
pub(super) fn as_of(value: LineCounts, as_of_ms: i64) -> Observed<LineCounts> {
    Observed::AsOf { value, as_of_ms }
}

/// What a client lists as it connects: the observations, and the list's revision.
pub(super) async fn listed(hub: &Hub) -> (Vec<Observation>, Rev) {
    let records = hub.records_list().await.unwrap();
    (records.observations, records.rev)
}

/// What the feed has told since it was last read.
pub(super) fn told(feed: &mut broadcast::Receiver<HubNotice>) -> Vec<HubNotice> {
    std::iter::from_fn(|| feed.try_recv().ok()).collect()
}

/// The one notice the feed has told since it was last read, which is a copy of an observation.
pub(super) fn told_copy(feed: &mut broadcast::Receiver<HubNotice>) -> Observation {
    match told(feed).as_slice() {
        [HubNotice::Observation(observation)] => observation.clone(),
        told => panic!("not one copy of an observation: {told:?}"),
    }
}
