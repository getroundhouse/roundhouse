use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::ToolServerSaveId;
use tokio::sync::watch;

/// The saves of tool servers under way, by the IDs their clients gave them, for a cancel to find while the server is
/// asked for its tools. No other client is told an ID, so none can cancel another's. They're in memory, since none
/// outlasts its connection or the hub.
#[derive(Default)]
pub(super) struct Saves {
    by_id: Mutex<HashMap<ToolServerSaveId, Arc<Save>>>,
}

/// One that's under way, and how far it has got.
pub(super) struct Save(watch::Sender<Stands>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stands {
    /// The hub hasn't begun to save it, as while its server is asked.
    Waiting,
    Cancelled,
    /// The hub has begun to save it, so a cancel is too late.
    Keeping,
}

impl Saves {
    /// Has the save `id` under way. Refused with `Conflict` while one of the same ID is.
    pub(super) fn begin(&self, id: &ToolServerSaveId) -> Result<Arc<Save>, ApiError> {
        let mut by_id = self.by_id.lock();
        if by_id.contains_key(id) {
            return Err(ApiError::new(ErrorCode::Conflict, "a save of an MCP server with that ID is under way"));
        }
        let save = Arc::new(Save(watch::Sender::new(Stands::Waiting)));
        by_id.insert(id.clone(), Arc::clone(&save));
        Ok(save)
    }

    /// Lets go of the save `id`, unless the hub has begun to save it. One that isn't under way has ended already.
    pub(super) fn cancel(&self, id: &ToolServerSaveId) {
        if let Some(save) = self.by_id.lock().get(id) {
            save.0.send_if_modified(|stands| {
                let waiting = *stands == Stands::Waiting;
                if waiting {
                    *stands = Stands::Cancelled;
                }
                waiting
            });
        }
    }

    /// Forgets `save`, as `id`, once it has ended.
    pub(super) fn end(&self, id: &ToolServerSaveId, save: &Arc<Save>) {
        self.by_id.lock().retain(|held, under_way| held != id || !Arc::ptr_eq(under_way, save));
    }
}

impl Save {
    /// Once it has been cancelled, which may be already.
    pub(super) async fn cancelled(&self) {
        let mut stands = self.0.subscribe();
        // Its sender is this, which outlives the wait.
        let _ = stands.wait_for(|stands| *stands == Stands::Cancelled).await;
    }

    /// Whether the hub is to save it, which it is unless it was cancelled first. From now on, a cancel is too late.
    pub(super) fn keep(&self) -> bool {
        let mut kept = false;
        self.0.send_if_modified(|stands| {
            if *stands == Stands::Waiting {
                *stands = Stands::Keeping;
            }
            kept = *stands == Stands::Keeping;
            // Nothing waits for it to be kept.
            false
        });
        kept
    }
}
