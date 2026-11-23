//! Starting and ending what a connection follows: a session's log, which [`super::pump`] feeds, or the hub's feed of
//! its records, which is fed from here. Each subscription has its entries in the hub's list of who follows what while
//! it lasts.

use std::sync::Arc;

use protocol::client::github::GithubShown;
use protocol::client::{ApiError, ErrorCode, HubFrame, HubNotice};
use protocol::model::SessionId;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::AbortHandle;

use super::pump::{Following, pump};
use super::{Connection, Outgoing};
use crate::hub::{Followed, FollowerId, Hub};
use crate::server::sends::Sends;

/// One subscription of a connection, which ends as it's dropped: its task stops, and its entries leave the hub's list of
/// who follows what.
pub(super) struct Subscription {
    /// What sends what it follows.
    task: AbortHandle,
    pub(super) following: crate::hub::Following,
    /// For a subscription to what GitHub says of what the client shows, what it shows, which the client can change.
    pub(super) shown: Option<watch::Sender<GithubShown>>,
    /// What it follows, for one that `Subscribe` started, so a start sent again can be told from one that takes its ID.
    pub(super) followed: Option<Followed>,
}

impl Subscription {
    /// Subscription `subscription_id` of `connection`, fed by `task`, which follows each of `followed` from now on.
    pub(super) fn new(
        hub: &Arc<Hub>,
        connection: &Connection,
        subscription_id: u64,
        followed: impl IntoIterator<Item = Followed>,
        task: AbortHandle,
    ) -> Self {
        let following = hub.follow(FollowerId { connection: connection.id, subscription: subscription_id }, followed);
        Self { task, following, shown: None, followed: None }
    }

    /// The same, for one to what GitHub says of what the client shows, which the client changes through `shown`.
    pub(super) fn showing(mut self, shown: watch::Sender<GithubShown>) -> Self {
        self.shown = Some(shown);
        self
    }

    /// The same, for one that `Subscribe` started, which follows `followed`.
    pub(super) fn of_subscribe(mut self, followed: Followed) -> Self {
        self.followed = Some(followed);
        self
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Starts feeding a subscription to a session's log.
pub(super) async fn follow_log(
    hub: &Arc<Hub>,
    connection: &Connection,
    id: SessionId,
    subscription_id: u64,
    following: Following,
) -> Result<(), ApiError> {
    let session = hub.live_session(&id).await?;
    add_subscription(hub, connection, subscription_id, Followed::Session(id.clone()), || {
        let live = session.subscribe();
        let frames = connection.frames.clone();
        tokio::spawn(pump(hub.clone(), session.clone(), id, subscription_id, following, live, frames)).abort_handle()
    })
}

/// Starts feeding a subscription to the hub's feed.
pub(super) fn follow_feed(hub: &Arc<Hub>, connection: &Connection, subscription_id: u64) -> Result<(), ApiError> {
    add_subscription(hub, connection, subscription_id, Followed::Hub, || {
        tokio::spawn(pump_feed(hub.subscribe_feed(), subscription_id, connection.frames.clone())).abort_handle()
    })
}

/// Ends any kind of subscription. One that isn't there is already ended.
pub(super) fn unsubscribe(connection: &Connection, subscription_id: u64) {
    // Dropped once the lock is let go, since its entries leaving the hub's list may tell a worker.
    let ended = connection.subscriptions.lock().remove(&subscription_id);
    drop(ended);
}

/// Starts a subscription that follows `followed`, with the task `start` starts. Holds the lock while `start` runs, so
/// two requests can't both take one subscription ID.
pub(super) fn add_subscription(
    hub: &Arc<Hub>,
    connection: &Connection,
    subscription_id: u64,
    followed: Followed,
    start: impl FnOnce() -> AbortHandle,
) -> Result<(), ApiError> {
    let mut subscriptions = connection.subscriptions.lock();
    if subscriptions.contains_key(&subscription_id) {
        return Err(taken(subscription_id));
    }
    let subscription = Subscription::new(hub, connection, subscription_id, [followed], start());
    subscriptions.insert(subscription_id, subscription);
    Ok(())
}

/// The refusal of a subscription whose ID another subscription of the connection has.
pub(super) fn taken(subscription_id: u64) -> ApiError {
    ApiError::new(ErrorCode::InvalidRequest, format!("subscription {subscription_id} already exists"))
}

/// Sends subscription `subscription_id` each notice of `feed` that `FeedSubscribe` is sent, as [`Sends::feed`] says.
async fn pump_feed(
    mut feed: broadcast::Receiver<Arc<HubNotice>>,
    subscription_id: u64,
    frames: mpsc::Sender<Outgoing>,
) {
    let sends = Sends::feed();
    loop {
        let notice = match feed.recv().await {
            Ok(notice) if sends.notice(&notice) => Arc::unwrap_or_clone(notice),
            Ok(_) => continue,
            Err(broadcast::error::RecvError::Lagged(_)) => HubNotice::Resync,
            Err(broadcast::error::RecvError::Closed) => return,
        };
        if frames.send(HubFrame::Notice { subscription_id, notice }.into()).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use protocol::client::github::{GithubShown, GithubSubscribe};
    use protocol::client::subscriptions::{FeedSubscribe, SessionSubscribe, SessionUnsubscribe};
    use protocol::client::terminals::TerminalWatch;
    use protocol::client::{CLIENT_PROTOCOL_VERSION, ClientResponse, Outcome, Request};
    use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk, Seq};

    use super::super::tests::{Connection, WAIT};
    use crate::hub::Followed;
    use crate::hub::tests::{hub_with_worktree, taking_worker, terminal_in_w};

    /// What each of the requests that follow something follows is in the hub's list of who follows what, until its
    /// subscription ends, or its connection does.
    #[tokio::test]
    async fn what_each_subscription_follows_is_in_the_list_until_it_ends() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_with_worktree(dir.path());
        let _worker = taking_worker(&hub);
        hub.terminal_open(terminal_in_w("t")).await.unwrap();
        let chip = ReferenceAsk {
            repository: GithubRepositoryName::named("example/project"),
            number: IssueNumber::try_from(12).unwrap(),
        };
        let shown =
            GithubShown { workspaces: vec!["w".into()], closely: vec!["w".into()], references: vec![chip.clone()] };
        let mut connection = Connection::greeted(&hub, CLIENT_PROTOCOL_VERSION).await;
        let followed = [
            Followed::Hub,
            Followed::Session("s".into()),
            Followed::Terminal("t".into()),
            Followed::Workspace("w".into()),
            Followed::Reference(chip),
        ];
        let requests = [
            FeedSubscribe { subscription_id: 1 }.into_request(),
            SessionSubscribe { id: "s".into(), from_seq: Seq::FIRST, subscription_id: 2 }.into_request(),
            TerminalWatch { id: "t".into(), subscription_id: 3 }.into_request(),
            GithubSubscribe { subscription_id: 4, shown }.into_request(),
        ];

        for (id, request) in (1..).zip(&requests) {
            assert_eq!(connection.reply(id, request).await, Outcome::Response(ClientResponse::Ok), "{request:?}");
        }
        let all_followed = followed.iter().all(|followed| hub.followed(followed));
        connection.reply(5, &SessionUnsubscribe { subscription_id: 2 }.into_request()).await;
        let after_one_ended = followed.iter().map(|followed| hub.followed(followed)).collect::<Vec<_>>();
        drop(connection);

        assert!(all_followed);
        assert_eq!(after_one_ended, [true, false, true, true, true]);
        waiting::task::until(WAIT, "the connection's followings to end", || {
            !followed.iter().any(|followed| hub.followed(followed))
        })
        .await;
    }
}
