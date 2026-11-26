use std::sync::Arc;

use protocol::client::{ApiError, ErrorCode, error_chain};
use protocol::model::ToolServer;
use protocol::worker::{ProxiedServer, RenewToolServer};
use tool_sign_in::Renewal;
use tracing::{error, warn};

use super::super::wall_clock::check_by_the_wall_clock;
use super::super::{Hub, off_thread};
use super::renewing::{Joined, UnderWay};
use crate::store::now_ms;
use crate::tool_servers::{Checked, Due, ToolServerChange};

impl Hub {
    /// Checks the sign-ins to tool servers as the hub starts and every
    /// [`check_tool_sign_ins_every`](crate::Timers::check_tool_sign_ins_every) after, as [`Hub::check_tool_sign_ins`]
    /// says, until the task it runs on is ended, as the hub stops.
    pub(crate) async fn watch_tool_sign_ins(self: Arc<Self>) {
        check_by_the_wall_clock(self.timers.check_tool_sign_ins_every, now_ms, std::future::pending(), |now_ms| {
            let hub = Arc::clone(&self);
            async move { hub.check_tool_sign_ins(now_ms).await }
        })
        .await;
    }

    /// Checks the sign-ins to tool servers at `now_ms` by the wall clock, as `ToolServers::check_sign_ins` says, tells
    /// every client of each it found ended, and starts renewing each it found due. A check waits for no renewal, so one
    /// that's slow holds up no other's.
    pub(super) async fn check_tool_sign_ins(self: &Arc<Self>, now_ms: i64) {
        let servers = Arc::clone(&self.tool_servers);
        let Checked { due, changed } = match off_thread(move || servers.check_sign_ins(now_ms)).await {
            Ok(checked) => checked,
            Err(e) => {
                error!(name: "tool_sign_in.check_failed", error = e.message, "Couldn't check the sign-ins to tool servers");
                return;
            }
        };
        self.announce_tool_servers(changed);
        for due in due {
            if self.tool_sign_ins.renewing.has_waited(&due, now_ms) {
                self.renew_tool_sign_in(due);
            }
        }
    }

    /// What a worker is to call a tool server with that refused its access token, as [`RenewToolServer`] says. One that
    /// asks as the token is being renewed waits for that, and one whose token was renewed moments ago has what that came
    /// to. `NotFound` when the server has been removed.
    pub(crate) async fn tool_server_refused(
        self: &Arc<Self>,
        asked: RenewToolServer,
    ) -> Result<ProxiedServer, ApiError> {
        let RenewToolServer { server, refused } = asked;
        loop {
            let (servers, id) = (Arc::clone(&self.tool_servers), server.clone());
            let Checked { due, changed } = off_thread(move || servers.refused(&id, refused)).await?;
            self.announce_tool_servers(changed);
            let Some(due) = due.into_iter().next() else { break };
            match self.tool_sign_ins.renewing.join(&due, now_ms()) {
                Joined::Begun(under_way, ends) => {
                    self.start_renewal_of(due, under_way);
                    ends.ended().await;
                    break;
                }
                // It may be of tokens the server had before it was signed in to again, so this looks again.
                Joined::UnderWay(ends) => ends.ended().await,
                Joined::Nothing => break,
            }
        }
        let gone = || ApiError::new(ErrorCode::NotFound, "the hub has no such MCP server to send a worker");
        self.tool_servers.sent_server(&server).ok_or_else(gone)
    }

    fn announce_tool_servers(&self, changed: Vec<ToolServer>) {
        for server in changed {
            self.announce_tool_server(ToolServerChange { server, changed: true });
        }
    }

    /// Starts renewing `due`, unless a renewal of it is under way.
    pub(super) fn renew_tool_sign_in(self: &Arc<Self>, due: Due) {
        if let Some(under_way) = self.tool_sign_ins.renewing.begin(&due.server) {
            self.start_renewal_of(due, under_way);
        }
    }

    /// Renews `due` on a task of its own, so nothing that asked for it cuts it off: the authorization server may take no
    /// refresh token but the one it gives.
    fn start_renewal_of(self: &Arc<Self>, due: Due, under_way: UnderWay) {
        let hub = Arc::clone(self);
        tokio::spawn(async move { hub.renewal_of(due, under_way).await });
    }

    /// Renews `due`: asks its authorization server, within its time limits, and then keeps what it answered, and tells
    /// every client when that changed the server's record. One that fails as may pass is left as it is, and tried again
    /// later.
    async fn renewal_of(&self, due: Due, _until_it_ends: UnderWay) {
        let due = Arc::new(due);
        let (servers, asking) = (Arc::clone(&self.tool_servers), Arc::clone(&due));
        // One renewed or signed in to again since it was found has another refresh token, and may be refused with this.
        match off_thread(move || servers.holds(&asking)).await {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => return self.not_renewed(&due, &e),
        }
        let renewal = match self.tool_sign_ins.client.renew(&due.renewable, &due.refresh).await {
            Ok(renewal) => renewal,
            Err(e) => return self.not_renewed(&due, &e),
        };
        let renewed = matches!(renewal, Renewal::Renewed(_));
        let (servers, renewing, answered_at_ms) = (Arc::clone(&self.tool_servers), Arc::clone(&due), now_ms());
        match off_thread(move || servers.renewed(&renewing, renewal, answered_at_ms)).await {
            Ok(Some(change)) => {
                if renewed {
                    self.tool_sign_ins.renewing.renewed(&due, answered_at_ms);
                } else {
                    self.tool_sign_ins.renewing.gone(&due.server);
                }
                self.announce_tool_server(change);
            }
            Ok(None) => self.tool_sign_ins.renewing.gone(&due.server),
            Err(e) => self.not_renewed(&due, &e),
        }
    }

    fn not_renewed(&self, due: &Due, error: &dyn std::error::Error) {
        warn!(
            name: "tool_sign_in.renewal_failed",
            server_id = %due.server,
            error = error_chain(error),
            "Couldn't renew the sign-in to a tool server, so it's tried again later"
        );
        self.tool_sign_ins.renewing.failed(due, now_ms());
    }

    /// Starts no renewal of a tool server's sign-in after, as the hub stops, and waits for those under way, before the
    /// hub lets go of its lock. An authorization server may take no refresh token but the one it gives, so a stop that
    /// dropped one would have the server need signing in to, where waiting makes the stop longer by at most the time
    /// limits on asking, about 30 s. A hub that's killed meanwhile still loses it.
    pub(crate) async fn stop_renewing_tool_sign_ins(&self) {
        self.tool_sign_ins.renewing.stop().await;
    }
}
