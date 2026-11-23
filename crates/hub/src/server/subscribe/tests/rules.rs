//! The rules of the request itself: what it refuses, what it does sent again, and its order.

use protocol::client::subscriptions::{Follow, LogFrom, Refused, Subscribe, Subscribed};
use protocol::client::{ClientResponse, ErrorCode, HubFrame, Request};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{Place, RunId, Seq};

use super::given::{Given, ok, start};
use crate::hub::Followed;
use crate::hub::tests::flushed;
use crate::server::tests::Connection;
use crate::session::Item;

/// A start sent again changes nothing and sends nothing again, one under an ID that follows something else is refused
/// alone, and one whose ID a stop in the same request lets go of is made.
#[tokio::test]
async fn a_start_sent_again_changes_nothing_and_one_under_a_taken_id_is_refused_alone() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let mut one = Connection::greeted(&given.hub, 4).await;
    let hub = || Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Hub)] };
    let taken = Subscribe {
        stop: Vec::new(),
        start: vec![start(1, Follow::Workspace { id: "w".into() }), start(2, Follow::Hub)],
    };
    let stopped_first = Subscribe { stop: vec![2], start: vec![start(2, Follow::Workspace { id: "w".into() })] };

    let first = ok(&mut one, 1, hub()).await;
    let again = ok(&mut one, 2, hub()).await;
    let ClientResponse::Subscribed(Subscribed { refused }) = ok(&mut one, 3, taken).await else { panic!() };
    let replaced = ok(&mut one, 4, stopped_first).await;
    ok(&mut one, 5, protocol::client::admin::HubStatus).await;

    assert_eq!((&first, &again), (&ClientResponse::Subscribed(Subscribed::default()), &first));
    assert_eq!(
        refused.iter().map(|refused| (refused.subscription_id, refused.error.code)).collect::<Vec<_>>(),
        [(1, ErrorCode::InvalidRequest)]
    );
    assert_eq!(replaced, first);
    let listed = |id: u64| {
        one.sent
            .iter()
            .filter(|frame| matches!(frame, HubFrame::Listed { subscription_id, .. } if *subscription_id == id))
            .count()
    };
    assert_eq!(
        (listed(1), listed(2)),
        (1, 1),
        "the hub's records, listed once for each that started at the hub's level"
    );
    assert!(given.hub.followed(&Followed::Workspace("w".into())));
}

/// What the hub doesn't know, or doesn't have, is refused for that start alone, and the rest are made.
#[tokio::test]
async fn what_the_hub_doesnt_know_or_have_is_refused_and_the_rest_are_made() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let mut one = Connection::greeted(&given.hub, 4).await;
    let starts = vec![
        start(1, Follow::Unknown),
        start(2, Follow::Session { id: "s".into(), from: Some(LogFrom::Unknown) }),
        start(3, Follow::Session { id: "nowhere".into(), from: None }),
        start(4, Follow::Workspace { id: "nowhere".into() }),
        start(5, Follow::Terminal { id: "nowhere".into() }),
        start(6, Follow::Terminal { id: "t".into() }),
    ];

    let ClientResponse::Subscribed(Subscribed { refused }) =
        ok(&mut one, 1, Subscribe { stop: Vec::new(), start: starts }).await
    else {
        panic!()
    };

    let refused: Vec<_> =
        refused.iter().map(|Refused { subscription_id, error }| (*subscription_id, error.code)).collect();
    let (unsupported, not_found) = (ErrorCode::Unsupported, ErrorCode::NotFound);
    assert_eq!(refused, [(1, unsupported), (2, unsupported), (3, not_found), (4, not_found), (5, not_found)]);
    assert!(given.hub.followed(&Followed::Terminal("t".into())));
}

/// A connection's `Subscribe` requests are taken in the order they were sent, so a stop sent straight after its start,
/// before the start's reply, leaves nothing followed.
#[tokio::test]
async fn a_stop_sent_straight_after_its_start_leaves_nothing_followed() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let mut one = Connection::greeted(&given.hub, 4).await;
    let started = Subscribe { stop: Vec::new(), start: vec![start(1, Follow::Workspace { id: "w".into() })] };

    for _ in 0..20 {
        one.write(1, &started.clone().into_request()).await;
        one.write(2, &Subscribe { stop: vec![1], start: Vec::new() }.into_request()).await;
        one.reply_to(1).await;
        one.reply_to(2).await;

        assert!(!given.hub.followed(&Followed::Workspace("w".into())));
    }
}

/// A session followed from a `seq`, or from the agent's own place, is sent only its events from there.
#[tokio::test]
async fn a_session_followed_from_where_a_client_got_to_is_sent_its_events_from_there() {
    let dir = tempfile::tempdir().unwrap();
    let given = Given::new(dir.path()).await;
    let run = RunId::new(1);
    for text in ["one", "two", "three"] {
        let blocks = vec![ContentBlock::Text { text: text.into(), images: Vec::new() }];
        let body = EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None };
        given
            .session
            .send_item(Item::Event { run, body: Box::new(body), raw: None, attachments: Vec::new(), seq: None })
            .await;
    }
    flushed(&given.session).await;
    let mut one = Connection::greeted(&given.hub, 4).await;
    let from = |subscription_id, from| start(subscription_id, Follow::Session { id: "s".into(), from: Some(from) });
    let starts = vec![
        from(1, LogFrom::Seq { seq: Seq::new(2) }),
        from(2, LogFrom::Place { subagent: None, place: Place::new(3) }),
    ];

    ok(&mut one, 1, Subscribe { stop: Vec::new(), start: starts }).await;

    let tails = |sent: &[HubFrame]| sent.iter().filter(|frame| matches!(frame, HubFrame::Tail { .. })).count();
    one.until("both to catch up", |sent| tails(sent) == 2).await;
    let sent_on = |id: u64| {
        let events = one.sent.iter().filter_map(|frame| {
            let HubFrame::Event { subscription_id, event } = frame else { return None };
            (*subscription_id == id).then(|| event.seq.get())
        });
        events.collect::<Vec<_>>()
    };
    assert_eq!((sent_on(1), sent_on(2)), (vec![2, 3], vec![3]));
    assert!(given.hub.followed(&Followed::Session("s".into())));
}
