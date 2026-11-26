use std::sync::Arc;

use protocol::model::WorkspaceId;

use super::super::Hub;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Has each chat of `workspace_id` look at whether its agent is to connect to a tool server again, each time `link`,
    /// its worker's, has the token of one that's newly signed in to, until the link closes. A renewal is of the same
    /// sign-in, so it wakes none. From as it's called, rather than as it's first polled, so none is missed between.
    pub(crate) fn reconnect_as_signed_in(
        self: Arc<Self>,
        workspace_id: WorkspaceId,
        link: Arc<WorkerLink>,
    ) -> impl Future<Output = ()> + Send + use<> {
        let mut signed_in = link.signed_in();
        async move {
            loop {
                tokio::select! {
                    changed = signed_in.changed() => if changed.is_err() { return },
                    () = link.wait_closed() => return,
                }
                let live: Vec<_> = self.sessions.lock().live.values().cloned().collect();
                for session in live.iter().filter(|session| session.workspace_id == workspace_id) {
                    session.wake_reconnects();
                }
            }
        }
    }
}
