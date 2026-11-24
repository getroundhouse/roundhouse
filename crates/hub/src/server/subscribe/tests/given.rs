//! The hub, worker and connections the tests share, and what they read of what was sent.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscription};
use protocol::client::{ClientResponse, HubFrame, HubNotice, Outcome, Request};
use protocol::event::SessionEvent;
use protocol::model::{DiffComment, DiffSide, RunId};
use protocol::tail::Tail;
use protocol::terminal::{
    Numbering, PictureSending, RowJson, RowNumber, ScreenState, Shown, Span, TerminalPicture, TerminalRow, TerminalSize,
};
use protocol::worker::{WorkerRequest, WorkerResponse};

use crate::hub::Hub;
use crate::hub::tests::{carried_on, hub_with_worktree, serving, terminal_in_w, writing};
use crate::server::tests::{Connection, WAIT};
use crate::session::{Item, LiveSession};
use crate::worker_link::WorkerLink;
/// A hub as the module's doc says, and what its worker has been told of whether `t` is watched, in order.
pub(super) struct Given {
    pub(super) hub: Arc<Hub>,
    pub(super) link: Arc<WorkerLink>,
    pub(super) session: Arc<LiveSession>,
    pub(super) told: Arc<Mutex<Vec<bool>>>,
}

impl Given {
    pub(super) async fn new(dir: &std::path::Path) -> Self {
        let hub = hub_with_worktree(dir);
        let told = Arc::new(Mutex::new(Vec::new()));
        let telling = told.clone();
        let link = WorkerLink::answering(move |request| {
            if let WorkerRequest::TerminalWatch(asked) = &request {
                telling.lock().push(asked.watching);
            }
            Some(Ok(WorkerResponse::Ok))
        });
        serving(&hub, &link);
        hub.terminal_open(terminal_in_w("t")).await.unwrap();
        // Read once, so following `w` reads the same again, and changes nothing a client is sent.
        let _ = hub.workspace_pull_request("w".into()).await;
        let session = carried_on(&hub, &link, RunId::new(1));
        Self { hub, link, session, told }
    }

    pub(super) fn told(&self) -> Vec<bool> {
        self.told.lock().clone()
    }

    /// A change to a record of the hub's level, and to one of `s`'s own, what its agent is writing, and what `t` shows,
    /// once its worker has been told something watches it.
    pub(super) async fn changes(&self) {
        self.hub.notify(comment_made(&self.hub, "why"));
        self.hub.notify(subagent_made(&self.hub));
        self.session.send_item(Item::Tail { run: RunId::new(1), tail: writing(RunId::new(1), "Reading the") }).await;
        waiting::task::until(WAIT, "the worker to be told t is watched", || self.told().contains(&true)).await;
        let row = |text: &str| {
            RowJson::new(&TerminalRow {
                spans: vec![Span { text: text.into(), ..Span::default() }],
                ..TerminalRow::default()
            })
            .unwrap()
        };
        let shown = Shown {
            numbering: Numbering::new(1),
            size: TerminalSize { cols: 80, rows: 1 },
            top: RowNumber::new(200),
            first: RowNumber::new(100),
            rows: vec![row("$ ls")],
            cursor: None,
            state: ScreenState::default(),
        };
        let picture = PictureSending::default().next(shown, |_| None).unwrap();
        self.hub.terminal_picture(&self.link, &"t".into(), picture);
    }
}

/// What a connection was sent, by kind, whichever subscription carried it.
#[derive(Debug, Default, PartialEq)]
pub(super) struct ByKind {
    pub(super) notices: Vec<HubNotice>,
    pub(super) events: Vec<SessionEvent>,
    pub(super) tails: Vec<Tail>,
    pub(super) pictures: Vec<TerminalPicture>,
    pub(super) listed: Vec<Listed>,
}

impl ByKind {
    pub(super) fn of(sent: &[HubFrame]) -> Self {
        let mut kinds = Self::default();
        for frame in sent.iter().cloned() {
            match frame {
                HubFrame::Notice { notice, .. } => kinds.notices.push(notice),
                HubFrame::Event { event, .. } => kinds.events.push(event),
                HubFrame::Tail { tail, .. } => kinds.tails.push(tail),
                HubFrame::TerminalPicture { picture, .. } => kinds.pictures.push(picture),
                HubFrame::Listed { listed, .. } => kinds.listed.push(listed),
                frame @ (HubFrame::Reply { .. } | HubFrame::Delta { .. } | HubFrame::Terminal { .. }) => {
                    panic!("not sent here: {frame:?}")
                }
            }
        }
        kinds
    }
}

pub(super) async fn ok(connection: &mut Connection, id: u64, request: impl Request) -> ClientResponse {
    match connection.reply(id, &request.into_request()).await {
        Outcome::Response(response) => response,
        Outcome::Error(error) => panic!("call {id}: {error:?}"),
    }
}

pub(super) fn start(subscription_id: u64, follow: Follow) -> Subscription {
    Subscription::new(subscription_id, follow)
}

/// The comment `c` on `w`'s diff, saying `text`, as the hub would tell of it made now: at a revision newer than any it
/// has told of, so it's sent. A test sends it after something, as a marker that a client sent it has been sent that too.
pub(super) fn comment_made(hub: &Hub, text: &str) -> HubNotice {
    HubNotice::Comment(DiffComment {
        id: "c".into(),
        workspace_id: "w".into(),
        path: "a.rs".into(),
        side: DiffSide::New,
        line: 1,
        text: text.into(),
        created_at_ms: 1,
        rev: crate::hub::tests::store_of(hub).next_rev().unwrap(),
    })
}

/// The subagent `c` of `s`, as the hub would tell of it made now, as [`comment_made`] says.
pub(super) fn subagent_made(hub: &Hub) -> HubNotice {
    let rev = crate::hub::tests::store_of(hub).next_rev().unwrap();
    HubNotice::Subagent(protocol::model::Subagent { rev, ..protocol::fixtures::subagent("c") })
}

/// Waits until `client` is sent a notice `hub` tells after all it told before, so it has been sent everything before.
pub(super) async fn mark(hub: &crate::hub::Hub, client: &mut Connection) {
    let marker = comment_made(hub, "after");
    hub.notify(marker.clone());
    let at_the_marker = |sent: &[HubFrame]| {
        sent.iter().any(|frame| matches!(frame, HubFrame::Notice { notice, .. } if *notice == marker))
    };
    client.until("the marker", at_the_marker).await;
}
