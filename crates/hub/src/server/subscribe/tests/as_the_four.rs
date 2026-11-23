//! What the one request is sent, beside what the four requests from before it send.

use protocol::client::github::{GithubShown, GithubSubscribe};
use protocol::client::listed::{Listed, ReferenceObservations, SessionRecords, WorkspaceObservations};
use protocol::client::records::{RecordsList, SessionProposals, SessionRepliesHeard, SessionSubagents};
use protocol::client::subscriptions::{FeedSubscribe, Follow, SessionSubscribe, Subscribe, Subscribed};
use protocol::client::terminals::TerminalWatch;
use protocol::client::{ClientResponse, HubFrame, HubNotice};
use protocol::model::{Records, Rev, Seq};

use super::given::{ByKind, Given, ok, start};
use crate::hub::Followed;
use crate::server::tests::{Connection, WAIT};

fn following_hub_s_t_and_w() -> Subscribe {
    Subscribe {
        stop: Vec::new(),
        start: vec![
            start(1, Follow::Hub),
            start(2, Follow::Session { id: "s".into(), from: None }),
            start(3, Follow::Terminal { id: "t".into() }),
            start(4, Follow::Workspace { id: "w".into() }),
        ],
    }
}

/// Where the hub's records and `s`'s stand, as a client of version 3 lists them.
async fn listed_by_four(four: &mut Connection) -> [Listed; 2] {
    let ClientResponse::Records(records) = ok(four, 10, RecordsList).await else { panic!() };
    let ClientResponse::Subagents(subagents) = ok(four, 11, SessionSubagents { id: "s".into() }).await else {
        panic!()
    };
    let ClientResponse::Proposals(proposals) = ok(four, 12, SessionProposals { id: "s".into() }).await else {
        panic!()
    };
    let ClientResponse::RepliesHeard(heard) = ok(four, 13, SessionRepliesHeard { id: "s".into() }).await else {
        panic!()
    };
    let session = SessionRecords {
        session_id: "s".into(),
        subagents: subagents.subagents,
        proposals: proposals.proposals,
        replies_heard: heard.replies,
        rev: Rev::default(),
    };
    [Listed::Hub(Box::new(Records { rev: Rev::default(), ..records })), Listed::Session(session)]
}

/// The revisions lists were read at, which two read apart can differ in, though nothing they hold does.
fn without_revisions(listed: Listed) -> Listed {
    match listed {
        Listed::Hub(records) => Listed::Hub(Box::new(Records { rev: Rev::default(), ..*records })),
        Listed::Session(records) => Listed::Session(SessionRecords { rev: Rev::default(), ..records }),
        Listed::Workspace(observations) => {
            Listed::Workspace(WorkspaceObservations { rev: Rev::default(), ..observations })
        }
        Listed::Reference(observed) => Listed::Reference(ReferenceObservations { rev: Rev::default(), ..observed }),
        Listed::Unknown => Listed::Unknown,
    }
}

/// A client that follows the hub, a session, a terminal and a workspace through one `Subscribe` is sent what the four
/// requests send a client of version 3, with where the hub's records and the session's stand in place of their lists,
/// and the session's own records on its subscription rather than the hub's. The workspace's observations, of which `w`
/// has none, are listed too, which the four don't send.
#[tokio::test]
async fn following_through_the_one_request_is_sent_what_the_four_requests_send() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let mut four = Connection::greeted(&given.hub, 3).await;
    ok(&mut four, 1, FeedSubscribe { subscription_id: 1 }).await;
    ok(&mut four, 2, SessionSubscribe { id: "s".into(), from_seq: Seq::FIRST, subscription_id: 2 }).await;
    ok(&mut four, 3, TerminalWatch { id: "t".into(), subscription_id: 3 }).await;
    let shown = GithubShown { workspaces: vec!["w".into()], closely: vec!["w".into()], references: Vec::new() };
    ok(&mut four, 4, GithubSubscribe { subscription_id: 4, shown }).await;
    let listed = listed_by_four(&mut four).await;
    let mut one = Connection::greeted(&given.hub, 4).await;
    let subscribed = ok(&mut one, 1, following_hub_s_t_and_w()).await;

    given.changes().await;

    four.until("what the four requests send", |sent| ByKind::of(sent).has_all()).await;
    one.until("what the one request sends", |sent| ByKind::of(sent).has_all()).await;
    let (four, one_sent) = (ByKind::of(&four.sent), one.sent.clone());
    let mut one = ByKind::of(&one_sent);
    assert_eq!(subscribed, ClientResponse::Subscribed(Subscribed::default()));
    let (workspace, levels): (Vec<_>, Vec<_>) = std::mem::take(&mut one.listed)
        .into_iter()
        .map(without_revisions)
        .partition(|listed| matches!(listed, Listed::Workspace(_)));
    assert_eq!(levels, listed);
    let none = WorkspaceObservations { workspace_id: "w".into(), observations: Vec::new(), rev: Rev::default() };
    assert_eq!(workspace, [Listed::Workspace(none)]);
    assert_eq!(one, four);
    let session_record_at_hub_level = one_sent
        .iter()
        .any(|frame| matches!(frame, HubFrame::Notice { subscription_id: 1, notice: HubNotice::Subagent(_) }));
    assert!(!session_record_at_hub_level, "{one_sent:#?}");
}

/// A worker stops sending what its terminal shows once nothing follows it, whichever request followed it, and whether a
/// subscription stopped or its connection ended.
#[tokio::test]
async fn a_worker_stops_sending_a_terminals_screen_once_no_client_follows_it() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let mut four = Connection::greeted(&given.hub, 3).await;
    let mut one = Connection::greeted(&given.hub, 4).await;
    ok(&mut four, 1, TerminalWatch { id: "t".into(), subscription_id: 1 }).await;
    ok(&mut one, 1, Subscribe { stop: Vec::new(), start: vec![start(7, Follow::Terminal { id: "t".into() })] }).await;
    waiting::task::until(WAIT, "the worker to be told t is watched", || given.told() == [true]).await;

    ok(&mut one, 2, Subscribe { stop: vec![7], start: Vec::new() }).await;
    let told_while_one_follows = given.told();
    drop(four);

    waiting::task::until(WAIT, "the worker to be told nothing watches t", || given.told() == [true, false]).await;
    assert_eq!(told_while_one_follows, [true]);
    assert!(!given.hub.followed(&Followed::Terminal("t".into())));
}
