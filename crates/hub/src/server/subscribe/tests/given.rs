//! The hub, worker and connections the tests share, and what they read of what was sent.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::client::github::GithubUpdate;
use protocol::client::listed::Listed;
use protocol::client::subscriptions::{Follow, Subscription};
use protocol::client::{ClientResponse, HubFrame, HubNotice, Outcome, Request};
use protocol::event::SessionEvent;
use protocol::model::{DiffComment, DiffSide, Rev, RunId};
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
        self.hub.notify(HubNotice::Comment(DiffComment {
            id: "c".into(),
            workspace_id: "w".into(),
            path: "a.rs".into(),
            side: DiffSide::New,
            line: 1,
            text: "why".into(),
            created_at_ms: 0,
            rev: Rev::default(),
        }));
        self.hub.notify(HubNotice::Subagent(protocol::fixtures::subagent("c")));
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
    pub(super) github: Vec<GithubUpdate>,
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
                HubFrame::Github { update, .. } => kinds.github.push(update),
                HubFrame::Listed { listed, .. } => kinds.listed.push(listed),
                frame @ (HubFrame::Reply { .. } | HubFrame::Delta { .. } | HubFrame::Terminal { .. }) => {
                    panic!("not sent here: {frame:?}")
                }
            }
        }
        kinds
    }

    /// Whether it holds all that [`Given::changes`] and following `w`, `s` and `t` send.
    pub(super) fn has_all(&self) -> bool {
        let of = |kind: fn(&HubNotice) -> bool| self.notices.iter().filter(|notice| kind(notice)).count();
        of(|notice| matches!(notice, HubNotice::Comment(_))) == 1
            && of(|notice| matches!(notice, HubNotice::Subagent(_))) == 1
            && self.tails.len() == 2
            && self.pictures.len() == 1
            && self.github.len() == 1
    }
}

pub(super) async fn ok(connection: &mut Connection, id: u64, request: impl Request) -> ClientResponse {
    match connection.reply(id, &request.into_request()).await {
        Outcome::Response(response) => response,
        Outcome::Error(error) => panic!("call {id}: {error:?}"),
    }
}

pub(super) fn start(subscription_id: u64, follow: Follow) -> Subscription {
    Subscription { subscription_id, follow }
}
