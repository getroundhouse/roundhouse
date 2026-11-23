//! The one request that follows everything, `subscribe`, its reply, and the list each level sends first, which every
//! peer from now on reads as they were first written.

use protocol::client::listed::{Listed, SessionRecords, WorkspaceObservations};
use protocol::client::subscriptions::{Follow, LogFrom, Refused, Subscribe, Subscribed, Subscription};
use protocol::client::{ApiError, ClientRequest, ClientResponse, ErrorCode, HubFrame};
use protocol::frame;
use protocol::model::{
    GithubRepositoryName, IssueNumber, LineCounts, Observation, Observed, Place, Records, ReferenceAsk, Rev, Seq,
};
use serde::Deserialize;

use super::keeps_its_form;

fn start(subscription_id: u64, follow: Follow) -> Subscription {
    Subscription::new(subscription_id, follow)
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

/// A start as a hub from before `github_observed` reads one.
#[derive(serde::Deserialize, Debug, PartialEq)]
struct StartBefore {
    subscription_id: u64,
    #[serde(flatten)]
    follow: Follow,
}

/// A start that reads what GitHub says as observations says so, and one that doesn't, as every client from before it,
/// says nothing. A hub from before it reads the first as the start it was before.
#[test]
fn a_start_that_reads_github_as_observations_says_so() {
    let reference =
        ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() };
    let observed =
        Subscription { github_observed: true, ..start(3, Follow::Reference { reference: reference.clone() }) };
    let line = concat!(
        r#"{"method":"subscribe","start":[{"subscription_id":3,"level":"reference","#,
        r#""reference":{"repository":"o/r","number":41},"github_observed":true}]}"#
    );

    keeps_its_form(&ClientRequest::Subscribe(Subscribe { stop: Vec::new(), start: vec![observed] }), line);

    let before: StartBefore = serde_json::from_str(
        r#"{"subscription_id":3,"level":"reference","reference":{"repository":"o/r","number":41},"github_observed":true}"#,
    )
    .unwrap();
    assert_eq!(before, StartBefore { subscription_id: 3, follow: Follow::Reference { reference } });
    let hub: StartBefore =
        serde_json::from_str(r#"{"subscription_id":1,"level":"hub","github_observed":true}"#).unwrap();
    assert_eq!(hub.follow, Follow::Hub);
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
        observations: Vec::new(),
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
    keeps_its_form(
        &HubFrame::Listed { subscription_id: 4, listed: Listed::Workspace(workspace_listed()) },
        concat!(
            r#"{"frame":"listed","subscription_id":4,"listed":{"level":"workspace","workspace_id":"w","#,
            r#""observations":[{"kind":"workspace_lines","workspace_id":"w","lines":{"freshness":"not_known"},"#,
            r#""rev":3}],"rev":11}}"#
        ),
    );
    let newer =
        frame::decode::<HubFrame>(br#"{"frame":"listed","subscription_id":4,"listed":{"level":"galaxy","rev":1}}"#);
    assert_eq!(newer.unwrap(), HubFrame::Listed { subscription_id: 4, listed: Listed::Unknown });
}

/// A workspace's observations, as a list of them.
fn workspace_listed() -> WorkspaceObservations {
    let lines = Observed::<LineCounts>::NotKnown;
    let observation = Observation::WorkspaceLines { workspace_id: "w".into(), lines, rev: Rev::new(3) };
    WorkspaceObservations { workspace_id: "w".into(), observations: vec![observation], rev: Rev::new(11) }
}

/// Where a level stands as a client from before workspaces had a level reads it.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(tag = "level", rename_all = "snake_case")]
enum ListedBefore {
    Session {
        session_id: String,
    },
    #[serde(other)]
    Unknown,
}

/// A client from before workspaces had a level skips the list of one, as of a level it doesn't know.
#[test]
fn a_build_from_before_reads_a_workspace_s_list_as_unknown() {
    let listed = serde_json::to_string(&Listed::Workspace(workspace_listed())).unwrap();

    assert_eq!(serde_json::from_str::<ListedBefore>(&listed).unwrap(), ListedBefore::Unknown, "{listed}");
}
