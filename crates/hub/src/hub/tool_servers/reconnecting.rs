use std::sync::Arc;

use protocol::model::WorkspaceId;

use super::super::Hub;
use crate::worker_link::WorkerLink;

impl Hub {
    /// Has each chat of `workspace_id` look at whether its agent is to connect to a tool server again, each time `link`,
    /// its worker's, has one as it wasn't before, until the link closes. A renewal and a rename leave one as it was, so
    /// neither wakes any. From as it's called, rather than as it's first polled, so none is missed between.
    pub(crate) fn reconnect_as_changed(
        self: Arc<Self>,
        workspace_id: WorkspaceId,
        link: Arc<WorkerLink>,
    ) -> impl Future<Output = ()> + Send + use<> {
        let mut servers = link.tool_servers();
        async move {
            loop {
                tokio::select! {
                    changed = servers.changed() => if changed.is_err() { return },
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
