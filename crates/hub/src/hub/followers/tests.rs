use protocol::model::{GithubRepositoryName, IssueNumber, ReferenceAsk, WorkspaceId};

use super::Followed;
use crate::hub::tests::hub_in;

fn chip(repository: &'static str, number: u64) -> Followed {
    let repository = GithubRepositoryName::named(repository);
    Followed::Reference(ReferenceAsk { repository, number: IssueNumber::try_from(number).unwrap() })
}

/// Something is followed while any subscription follows it, and a subscription that says it follows something else
/// follows only that from then on.
#[tokio::test]
async fn something_is_followed_until_the_last_subscription_that_follows_it_lets_go() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());
    let (w, v) = (Followed::Workspace("w".into()), Followed::Workspace("v".into()));
    let first = hub.follow([Followed::Hub, w.clone()]);
    let mut second = hub.follow([w.clone()]);

    drop(first);
    let after_first = (hub.followed(&Followed::Hub), hub.followed(&w));
    second.replace([v.clone()]);
    let replaced = (hub.followed(&w), hub.followers.workspaces());
    drop(second);

    assert_eq!(after_first, (false, true), "the second still follows w");
    assert_eq!(replaced, (false, [WorkspaceId::from("v")].into()));
    assert!(!hub.followed(&v));
}

/// The issues and pull requests followed are each read once, however many follow them, and however each names its
/// repository, since GitHub's names ignore case.
#[tokio::test]
async fn an_issue_followed_twice_is_read_once() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_in(dir.path());

    let _following = hub.follow([chip("example/project", 12), chip("Example/Project", 12)]);
    let _again = hub.follow([chip("example/project", 12), chip("example/project", 13), Followed::Hub]);

    let mut numbers: Vec<u64> = hub.followers.references().into_iter().map(|(_, number)| number.get()).collect();
    numbers.sort_unstable();
    assert_eq!(numbers, [12, 13]);
}
