//! What the hub sends a client of version 4 of what GitHub says, which every peer from now on reads as it was first
//! written.

use protocol::client::HubFrame;
use protocol::client::github::GithubUpdate;
use protocol::frame;
use protocol::model::{
    ChecksRollup, GithubLoginKind, GithubRepositoryName, IssueNumber, LeftQueue, Local, OwnPullRequest, PullRequest,
    PullRequestFound, PullRequestNumber, QueueState, ReferenceAsk, ReferenceLabel, ReferenceRead, RepositoryUnseen,
    Review,
};

use super::keeps_its_form;

fn ask() -> ReferenceAsk {
    ReferenceAsk { repository: GithubRepositoryName::named("o/r"), number: IssueNumber::try_from(41).unwrap() }
}

/// Pull request #7 of example/project with something in every field of every facet, so each is written out.
pub(super) fn every_facet() -> PullRequest {
    let mut pull_request = protocol::fixtures::pull_request(7);
    let summary = &mut pull_request.summary.said;
    summary.about.author = Some("ada".into());
    summary.about.labels = vec![ReferenceLabel { name: "ui".into(), color: "0e8a16".into() }];
    (summary.review, summary.queue, summary.rollup) =
        (Review::Approved, Some(QueueState::AwaitingChecks), Some(ChecksRollup::Running));
    let history = &mut pull_request.history.as_mut().unwrap().said;
    history.changes_requested_at_ms = Some(3);
    history.left_queue = Some(LeftQueue { at_ms: 4, reason: "FAILED_CHECKS".into() });
    pull_request.stack.as_mut().unwrap().said.dependencies = vec![PullRequestNumber::try_from(6).unwrap()];
    pull_request
}

/// [`every_facet`], as every peer writes it, and the cache and the store keep it.
pub(super) const EVERY_FACET: &str = concat!(
    r#"{"repository":"example/project","number":7,"summary":{"said":{"about":{"#,
    r#""url":"https://github.com/example/project/pull/7","title":"Change 7","author":"ada","opened_at_ms":1,"#,
    r#""comments":0,"labels":[{"name":"ui","color":"0e8a16"}],"body":""},"state":"OPEN","is_draft":false,"#,
    r#""review":"APPROVED","queue":"AWAITING_CHECKS","merge_state":"CLEAN","mergeable":"mergeable","#,
    r#""base_branch":"main","head":"head-7","base":"base","added":0,"removed":0,"rollup":"running"},"#,
    r#""read_at_ms":1},"history":{"said":{"changes_requested_at_ms":3,"#,
    r#""left_queue":{"at_ms":4,"reason":"FAILED_CHECKS"}},"read_at_ms":1},"#,
    r#""stack":{"said":{"destination":"main","dependencies":[6],"linear":true},"read_at_ms":1},"#,
    r#""checks":{"said":{"revision":"head-7","requirements_known":true,"checks":[{"name":"checks","#,
    r#""source":"app:1","url":null,"state":"pending","required":true}]},"read_at_ms":1}}"#
);

#[test]
fn what_the_hub_sends_of_what_github_says_keeps_its_form() {
    let sent = |update| HubFrame::Github { subscription_id: 3, update };

    keeps_its_form(
        &sent(GithubUpdate::Reference { reference: ask(), read: ReferenceRead::Missing }),
        concat!(
            r#"{"frame":"github","subscription_id":3,"update":{"of":"referenced","#,
            r#""reference":{"repository":"o/r","number":41},"read":{"found":"missing"}}}"#
        ),
    );
    keeps_its_form(
        &sent(GithubUpdate::PullRequest {
            workspace: "w".into(),
            found: Some(PullRequestFound { pull_request: None }),
            unseen: None,
        }),
        r#"{"frame":"github","subscription_id":3,"update":{"of":"own_pull_request","workspace":"w","found":null}}"#,
    );
    let unseen = RepositoryUnseen {
        repository: GithubRepositoryName::named("o/r"),
        username: "ada".into(),
        login: GithubLoginKind::SignIn,
        installation: None,
        said: "Could not resolve to a Repository with the name 'o/r'.".into(),
        approve_at: Some("https://github.com/settings/connections/applications/an-app".into()),
    };
    keeps_its_form(
        &sent(GithubUpdate::PullRequest { workspace: "w".into(), found: None, unseen: Some(unseen) }),
        concat!(
            r#"{"frame":"github","subscription_id":3,"update":{"of":"own_pull_request","workspace":"w","unseen":"#,
            r#"{"repository":"o/r","username":"ada","login":"sign_in","#,
            r#""said":"Could not resolve to a Repository with the name 'o/r'.","#,
            r#""approve_at":"https://github.com/settings/connections/applications/an-app"}}}"#
        ),
    );
    let own = OwnPullRequest { pull_request: every_facet(), local: Local { uncommitted: 1, ahead: 2, behind: 0 } };
    keeps_its_form(
        &sent(GithubUpdate::PullRequest {
            workspace: "w".into(),
            found: Some(PullRequestFound { pull_request: Some(own) }),
            unseen: None,
        }),
        &format!(
            r#"{{"frame":"github","subscription_id":3,"update":{{"of":"own_pull_request","workspace":"w","found":{{"pull_request":{EVERY_FACET},"local":{{"uncommitted":1,"ahead":2,"behind":0}}}}}}}}"#
        ),
    );
    keeps_its_form(
        &sent(GithubUpdate::PullRequest { workspace: "w".into(), found: None, unseen: None }),
        r#"{"frame":"github","subscription_id":3,"update":{"of":"own_pull_request","workspace":"w"}}"#,
    );
}

/// What a newer hub sends of something this client doesn't know reads as unknown, for the client to skip, and so does
/// what a hub from before sent of a chip and of a workspace's pull request, in forms this client doesn't read.
#[test]
fn a_newer_or_older_hubs_update_reads_as_unknown() {
    let updates: [&[u8]; 3] = [
        br#"{"frame":"github","subscription_id":3,"update":{"of":"discussion","number":5}}"#,
        br#"{"frame":"github","subscription_id":3,"update":{"of":"reference","reference":{"repository":"o/r","number":41},"read":{"found":"missing"}}}"#,
        br#"{"frame":"github","subscription_id":3,"update":{"of":"pull_request","workspace":"w","found":null}}"#,
    ];

    for update in updates {
        let frame: HubFrame = frame::decode(update).unwrap();

        assert_eq!(frame, HubFrame::Github { subscription_id: 3, update: GithubUpdate::Unknown });
    }
}
