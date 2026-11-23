//! The one request that follows everything, `subscribe`, its reply, and the list each level sends first, which every
//! peer from now on reads as they were first written.

use protocol::client::listed::{Listed, SessionRecords};
use protocol::client::subscriptions::{Follow, LogFrom, Refused, Subscribe, Subscribed, Subscription};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame};
use protocol::frame;
use protocol::model::{GithubRepositoryName, IssueNumber, Place, Records, ReferenceAsk, Rev, Seq};

use super::keeps_its_form;

fn start(subscription_id: u64, follow: Follow) -> Subscription {
    Subscription { subscription_id, follow }
}

#[test]
fn subscribing_at_every_level_keeps_its_form() {
    let reference =
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() };
    let subscribe = Subscribe {
        stop: vec![1, 2],
        start: vec![
            start(3, Follow::Hub),
            start(4, Follow::Workspace { id: "w".into() }),
            start(5, Follow::Session { id: "s".into(), from: None }),
            start(6, Follow::Session { id: "s".into(), from: Some(LogFrom::Seq { seq: Seq::new(12) }) }),
            start(
                7,
                Follow::Session {
                    id: "s".into(),
                    from: Some(LogFrom::Place { subagent: Some("a".into()), place: Place::new(3) }),
                },
            ),
            start(
                8,
                Follow::Session { id: "s".into(), from: Some(LogFrom::Place { subagent: None, place: Place::new(4) }) },
            ),
            start(9, Follow::Terminal { id: "t".into() }),
            start(10, Follow::Reference { reference }),
        ],
    };

    keeps_its_form(
        &ClientRequest::Subscribe(subscribe),
        concat!(
            r#"{"method":"subscribe","stop":[1,2],"start":["#,
            r#"{"subscription_id":3,"level":"hub"},"#,
            r#"{"subscription_id":4,"level":"workspace","id":"w"},"#,
            r#"{"subscription_id":5,"level":"session","id":"s"},"#,
            r#"{"subscription_id":6,"level":"session","id":"s","from":{"by":"seq","seq":12}},"#,
            r#"{"subscription_id":7,"level":"session","id":"s","from":{"by":"place","subagent":"a","place":3}},"#,
            r#"{"subscription_id":8,"level":"session","id":"s","from":{"by":"place","place":4}},"#,
            r#"{"subscription_id":9,"level":"terminal","id":"t"},"#,
            r#"{"subscription_id":10,"level":"reference","reference":{"repository":"o/r","number":41}}]}"#
        ),
    );
    keeps_its_form(&ClientRequest::Subscribe(Subscribe::default()), r#"{"method":"subscribe"}"#);
}

/// What a newer client starts, at a level or from a place this build doesn't know, still reads, so the hub can refuse
/// that one start and make the rest.
#[test]
fn a_level_or_a_place_from_a_newer_client_reads_as_unknown() {
    let line = concat!(
        r#"{"method":"subscribe","start":[{"subscription_id":1,"level":"galaxy","id":"g"},"#,
        r#"{"subscription_id":2,"level":"session","id":"s","from":{"by":"timestamp","at_ms":1}}]}"#
    );

    let Ok(ClientRequest::Subscribe(Subscribe { start: starts, .. })) = frame::decode::<ClientRequest>(line.as_bytes())
    else {
        panic!("{line}")
    };

    let unknown_place = Follow::Session { id: "s".into(), from: Some(LogFrom::Unknown) };
    assert_eq!(starts, [start(1, Follow::Unknown), start(2, unknown_place)]);
}

#[test]
fn the_reply_says_which_starts_were_refused() {
    let refused = Refused { subscription_id: 4, error: ApiError::new(ErrorCode::NotFound, "no workspace w") };

    keeps_its_form(
        &ClientResponse::Subscribed(Subscribed { refused: vec![refused] }),
        r#"{"result":"subscribed","refused":[{"subscription_id":4,"error":{"code":"not_found","message":"no workspace w"}}]}"#,
    );
    keeps_its_form(&ClientResponse::Subscribed(Subscribed::default()), r#"{"result":"subscribed"}"#);
}

#[test]
fn where_each_level_stands_keeps_its_form() {
    let hub = Listed::Hub(Box::new(Records { rev: Rev::new(9), ..Records::default() }));
    let session = Listed::Session(SessionRecords {
        session_id: "s".into(),
        subagents: Vec::new(),
        proposals: Vec::new(),
        replies_heard: Vec::new(),
        rev: Rev::new(10),
    });

    keeps_its_form(
        &HubFrame::Listed { subscription_id: 3, listed: hub },
        concat!(
            r#"{"frame":"listed","subscription_id":3,"listed":{"level":"hub","repos":[],"workspaces":[],"sessions":[],"#,
            r#""comments":[],"hub_settings":null,"accounts":[],"new_workspace_draft":null,"github_accounts":[],"#,
            r#""github_sign_ins":[],"found_github_logins":[],"github_apps":[],"github_reach":[],"rev":9}}"#
        ),
    );
    keeps_its_form(
        &HubFrame::Listed { subscription_id: 5, listed: session },
        concat!(
            r#"{"frame":"listed","subscription_id":5,"listed":{"level":"session","session_id":"s","subagents":[],"#,
            r#""proposals":[],"replies_heard":[],"rev":10}}"#
        ),
    );
    let newer =
        frame::decode::<HubFrame>(br#"{"frame":"listed","subscription_id":4,"listed":{"level":"galaxy","rev":1}}"#);
    assert_eq!(newer.unwrap(), HubFrame::Listed { subscription_id: 4, listed: Listed::Unknown });
}
