//! A client's subscription to what GitHub says of what it shows, and what the hub sends of it, which every peer from now
//! on reads as they were first written.

use protocol::client::github::{GithubShown, GithubSubscribe, GithubUpdate};
use protocol::client::{ClientRequest, HubFrame};
use protocol::frame;
use protocol::model::{GithubRepositoryName, IssueNumber, PullRequestFound, ReferenceAsk, ReferenceRead};

use super::keeps_its_form;

fn ask() -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() }
}

#[test]
fn subscribing_to_what_github_says_keeps_its_form() {
    let shown =
        GithubShown { workspaces: vec!["w".into(), "v".into()], closely: vec!["w".into()], references: vec![ask()] };

    keeps_its_form(
        &ClientRequest::GithubSubscribe(GithubSubscribe { subscription_id: 3, shown }),
        concat!(
            r#"{"method":"github_subscribe","subscription_id":3,"#,
            r#""shown":{"workspaces":["w","v"],"closely":["w"],"references":[{"repository":"o/r","number":41}]}}"#
        ),
    );
    let ClientRequest::GithubSubscribe(nothing) =
        frame::decode(br#"{"method":"github_subscribe","subscription_id":3,"shown":{}}"#).unwrap()
    else {
        panic!()
    };
    assert_eq!(nothing.shown, GithubShown::default(), "what a client leaves out, it doesn't show");
}

#[test]
fn what_the_hub_sends_of_what_github_says_keeps_its_form() {
    let sent = |update| HubFrame::Github { subscription_id: 3, update };

    keeps_its_form(
        &sent(GithubUpdate::Reference { reference: ask(), read: ReferenceRead::Missing }),
        concat!(
            r#"{"frame":"github","subscription_id":3,"update":{"of":"reference","#,
            r#""reference":{"repository":"o/r","number":41},"read":{"found":"missing"}}}"#
        ),
    );
    keeps_its_form(
        &sent(GithubUpdate::PullRequest {
            workspace: "w".into(),
            found: Some(PullRequestFound { pull_request: None }),
        }),
        r#"{"frame":"github","subscription_id":3,"update":{"of":"pull_request","workspace":"w","found":null}}"#,
    );
    keeps_its_form(
        &sent(GithubUpdate::PullRequest { workspace: "w".into(), found: None }),
        r#"{"frame":"github","subscription_id":3,"update":{"of":"pull_request","workspace":"w"}}"#,
    );
}

/// What a newer hub sends of something this client doesn't know reads as unknown, for the client to skip.
#[test]
fn a_newer_hubs_update_reads_as_unknown() {
    let frame: HubFrame =
        frame::decode(br#"{"frame":"github","subscription_id":3,"update":{"of":"discussion","number":5}}"#).unwrap();

    assert_eq!(frame, HubFrame::Github { subscription_id: 3, update: GithubUpdate::Unknown });
}
