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

/// A pull request as a branch's are listed: its number, state, newest commit, the repository it's from and the one it
/// merges into.
fn node(number: u64, state: &str, from: Option<&str>, into: &str) -> Value {
    let from = from.map(|from| json!({ "nameWithOwner": from }));
    json!({"number": number, "state": state, "headRefOid": format!("head-{number}"), "headRepository": from,
        "baseRepository": {"nameWithOwner": into}})
}

/// What GitHub answers for a branch that's there, listing `nodes` as its own, and listing `by_name` by its name.
fn there(nodes: &[Value], by_name: &[Value]) -> (Value, Value) {
    (json!({"h": {"associatedPullRequests": {"nodes": nodes}}}), json!({"h": {"nodes": by_name}}))
}

async fn read(answers: Vec<Answer>, asks: &[HeadsAsk]) -> (HeadsRead, Vec<String>) {
    let github = StandIn::with(Script { graphql: answers, ..Script::default() }).await;
    let read = Github::new(GithubConfig::at(github.api()))
        .read_heads(&GithubToken::new("a-token"), asks, Leaving::ForOthers)
        .await;
    let queries = github.asked().into_iter().filter(|asked| asked.starts_with("post /graphql ")).collect();
    (read, queries)
}

/// Four branches in one query. Two are there, one of example/project and one of a fork, so each has its own pull
/// requests into example/project, however many forks' branches of the same name have one, and not one into another
/// repository. One is gone, as GitHub deletes one as its pull request merges, so its are those its name finds from
/// its own repository, but for one from a fork since deleted. And one is of example/gone, which isn't there.
#[tokio::test]
async fn a_branch_thats_there_has_its_own_pull_requests_and_one_thats_gone_those_its_name_finds() {
    let forks = || vec![node(10, "OPEN", Some("someone/project"), "example/project"); 5];
    let (own, own_by_name) = there(
        &[
            node(7, "MERGED", Some("example/project"), "example/project"),
            node(6, "OPEN", Some("example/project"), "example/other"),
        ],
        &forks(),
    );
    let (forked, forked_by_name) = there(&[node(8, "OPEN", Some("ada/project"), "example/project")], &forks());
    let gone_by_name = json!({"h": {"nodes": [node(9, "OPEN", Some("someone/project"), "example/project"),
        node(5, "MERGED", Some("example/project"), "example/project"), node(4, "CLOSED", None, "example/project")]}});
    let missing = json!({"type":"NOT_FOUND","path":["r3"],"message":"Could not resolve to a Repository."});
    let answer = answer_with(
        json!({"r0": own, "n0": own_by_name, "r1": forked, "n1": forked_by_name, "r2": {"h": null}, "n2": gone_by_name,
            "r3": null, "n3": null}),
        4900,
        &[missing],
    );
    let asks = [
        heads("example/project", "example/project", "birch"),
        heads("example/project", "ada/project", "birch"),
        heads("example/project", "example/project", "elm"),
        heads("example/gone", "example/gone", "birch"),
    ];

    let (read, queries) = read(vec![answer], &asks).await;

    let [own, forked, gone, missing] = <[_; 4]>::try_from(read.answers).unwrap();
    let numbers = |found: Vec<super::super::heads::Headed>| found.iter().map(|pr| pr.number.get()).collect::<Vec<_>>();
    let own = own.unwrap();
    assert_eq!(numbers(own.clone()), [7], "its own, into example/project, and no fork's");
    assert_eq!((own[0].state, own[0].head.clone()), (PrState::Merged, CommitId::from("head-7")));
    assert_eq!(numbers(forked.unwrap()), [8]);
    assert_eq!(numbers(gone.unwrap()), [5], "by its name, from its own repository");
    assert_eq!(missing.unwrap_err().code, ErrorCode::Unavailable);
    assert_eq!(queries.len(), 1, "{queries:?}");
    assert!(queries[0].contains(r#"r1: repository(owner:\"ada\",name:\"project\")"#), "{}", queries[0]);
    assert!(queries[0].contains(r#"ref(qualifiedName:\"refs/heads/birch\")"#), "{}", queries[0]);
    assert!(
        queries[0]
            .contains(r#"n2: repository(owner:\"example\",name:\"project\") { h: pullRequests(headRefName:\"elm\""#),
        "{}",
        queries[0]
    );
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
        let repositories: serde_json::Map<String, Value> = (from..to)
            .flat_map(|at| [(format!("r{at}"), there(&[], &[]).0), (format!("n{at}"), there(&[], &[]).1)])
            .collect();
        Value::Object(repositories)
    };
    let (read, queries) = read(vec![answer_with(empty(0, 50), 900, &[])], &asks).await;

    assert_eq!(queries.len(), 1, "the second waits for GitHub's limit to reset");
    assert!(read.answers[..50].iter().all(Result::is_ok));
    assert!(read.answers[50..].iter().all(|answer| answer.as_ref().is_err_and(|e| e.code == ErrorCode::Worker)));
    assert!(queries[0].contains("r49: repository(") && !queries[0].contains("r50: repository("));
}
