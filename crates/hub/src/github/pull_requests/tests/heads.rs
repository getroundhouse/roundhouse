//! Finding the pull requests of branches on GitHub, many in one query, each to its own answer.

use super::super::heads::{HeadsAsk, HeadsRead};
use super::reader::answer_with;
use super::*;
use crate::github::{Answer, Github, GithubConfig, Leaving, Script, StandIn};
use protocol::model::{BranchName, CommitId, GithubRepositoryName, GithubToken, PrState};

fn name(name: &str) -> GithubRepositoryName {
    GithubRepositoryName::try_from(name.to_string()).unwrap()
}

/// `branch` of `from`, into `into`.
fn heads(into: &str, from: &str, branch: &str) -> HeadsAsk {
    HeadsAsk { repository: name(into), from: name(from), branch: BranchName::from(branch) }
}

/// A pull request as a branch's are listed: its number, state, newest commit and the repository it's from.
fn node(number: u64, state: &str, from: Option<&str>) -> Value {
    let from = from.map(|from| json!({ "nameWithOwner": from }));
    json!({"number": number, "state": state, "headRefOid": format!("head-{number}"), "headRepository": from})
}

async fn read(answers: Vec<Answer>, asks: &[HeadsAsk]) -> (HeadsRead, Vec<String>) {
    let github = StandIn::with(Script { graphql: answers, ..Script::default() }).await;
    let read = Github::new(GithubConfig::at(github.api()))
        .read_heads(&GithubToken::new("a-token"), asks, Leaving::ForOthers)
        .await;
    let queries = github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).collect();
    (read, queries)
}

/// Two branches of example/project, one pushed to a fork, and one of example/gone, which isn't there, in one query.
/// A pull request from another repository's branch of the same name, or from a fork since deleted, isn't the
/// branch's.
#[tokio::test]
async fn branches_are_asked_after_together_and_only_pull_requests_from_their_own_repository_are_theirs() {
    let own = json!({"h": {"nodes": [node(9, "OPEN", Some("someone/project")), node(7, "MERGED", Some("example/project")),
        node(4, "CLOSED", None)]}});
    let forked = json!({"h": {"nodes": [node(8, "OPEN", Some("ada/project"))]}});
    let gone = json!({"type":"NOT_FOUND","path":["r2"],"message":"Could not resolve to a Repository."});
    let answer = answer_with(json!({"r0": own, "r1": forked, "r2": null}), 4900, &[gone]);
    let asks = [
        heads("example/project", "example/project", "birch"),
        heads("example/project", "ada/project", "birch"),
        heads("example/gone", "example/gone", "birch"),
    ];

    let (read, queries) = read(vec![answer], &asks).await;

    let [own, forked, gone] = <[_; 3]>::try_from(read.answers).unwrap();
    let numbers = |found: Vec<super::super::heads::Headed>| found.iter().map(|pr| pr.number.get()).collect::<Vec<_>>();
    let own = own.unwrap();
    assert_eq!(numbers(own.clone()), [7]);
    assert_eq!((own[0].state, own[0].head.clone()), (PrState::Merged, CommitId::from("head-7")));
    assert_eq!(numbers(forked.unwrap()), [8]);
    assert_eq!(gone.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(queries.len(), 1, "{queries:?}");
    assert!(queries[0].contains(r#"headRefName:\"birch\""#), "{}", queries[0]);
}

/// One about the query itself, rather than an ask, fails every ask in it.
#[tokio::test]
async fn an_error_about_the_whole_query_fails_every_branch_in_it() {
    let refused = json!({"type":"MAX_NODE_LIMIT_EXCEEDED","message":"too many nodes"});
    let answers = vec![answer_with(json!({}), 4900, &[refused])];
    let asks = [heads("example/project", "example/project", "birch"), heads("example/other", "example/other", "elm")];

    let (read, _) = read(answers, &asks).await;

    assert!(read.answers.iter().all(Result::is_err), "{:?}", read.answers);
}

/// More than a query carries, so a second query asks after the rest, and none once GitHub says its limit is down to
/// the part kept for others.
#[tokio::test]
async fn branches_past_what_a_query_carries_go_in_the_next_until_githubs_limit_is_nearly_spent() {
    let asks: Vec<HeadsAsk> = (0..60).map(|n| heads("example/project", "example/project", &format!("b{n}"))).collect();
    let empty = |from: usize, to: usize| {
        let repositories: serde_json::Map<String, Value> =
            (from..to).map(|at| (format!("r{at}"), json!({"h": {"nodes": []}}))).collect();
        Value::Object(repositories)
    };
    let (read, queries) = read(vec![answer_with(empty(0, 50), 900, &[])], &asks).await;

    assert_eq!(queries.len(), 1, "the second waits for GitHub's limit to reset");
    assert!(read.answers[..50].iter().all(Result::is_ok));
    assert!(read.answers[50..].iter().all(|answer| answer.as_ref().is_err_and(|e| e.code == ErrorCode::Worker)));
    assert!(queries[0].contains("r49: repository(") && !queries[0].contains("r50: repository("));
}
