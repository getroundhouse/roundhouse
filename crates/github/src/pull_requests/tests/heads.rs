//! Finding the pull requests of branches on GitHub, many in one query, each to its own answer.

use super::super::heads::{HeadsAsk, HeadsRead, HeadsUnread};
use super::reader::answer_with;
use super::*;
use crate::{Answer, Github, GithubConfig, Leaving, Script, StandIn};
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
    json!({"number": number, "url": format!("https://github.com/{into}/pull/{number}"), "state": state,
        "headRefOid": format!("head-{number}"), "headRepository": from, "baseRepository": {"nameWithOwner": into}})
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

/// Five branches in one query. Two are there: one of example/project, whose own pull requests are those listed,
/// however many forks' branches of the same name have one, and one of a clone of a fork, ada/project, whose own merge
/// into the repository it was forked from. One is gone, as GitHub deletes one as its pull request merges, so its are
/// those its name finds from its own repository, but for one from a fork since deleted. One was pushed to a fork
/// that's been deleted since, so GitHub has no repository to ask of the branch in, and its name finds none. One is of
/// example/gone, which isn't there, so neither can be asked. And one is of example/kept-out, whose organization keeps
/// the token's app out, so GitHub forbids both.
#[tokio::test]
async fn a_branch_thats_there_has_its_own_pull_requests_and_one_thats_gone_those_its_name_finds() {
    let forks = || vec![node(10, "OPEN", Some("someone/project"), "example/project"); 5];
    let (own, own_by_name) = there(&[node(7, "MERGED", Some("example/project"), "example/project")], &forks());
    let (forked, forked_by_name) = there(&[node(8, "OPEN", Some("ada/project"), "example/project")], &[]);
    let gone_by_name = json!({"h": {"nodes": [node(9, "OPEN", Some("someone/project"), "example/project"),
        node(5, "MERGED", Some("example/project"), "example/project"), node(4, "CLOSED", None, "example/project")]}});
    let fork_gone_by_name = json!({"h": {"nodes": [node(3, "MERGED", None, "example/project")]}});
    let not_found =
        |alias: &str| json!({"type":"NOT_FOUND","path":[alias],"message":"Could not resolve to a Repository."});
    let kept_out = "The `example` organization has enabled OAuth App access restrictions.";
    let forbidden = |alias: &str| json!({"type":"FORBIDDEN","path":[alias],"message":kept_out});
    let answer = answer_with(
        json!({"r0": own, "n0": own_by_name, "r1": forked, "n1": forked_by_name, "r2": {"h": null}, "n2": gone_by_name,
            "r3": null, "n3": fork_gone_by_name, "r4": null, "n4": null, "r5": null, "n5": null}),
        4900,
        &[not_found("r3"), not_found("r4"), not_found("n4"), forbidden("r5"), forbidden("n5")],
    );
    let asks = [
        heads("example/project", "example/project", "birch"),
        heads("ada/project", "ada/project", "birch"),
        heads("example/project", "example/project", "elm"),
        heads("example/project", "old/project", "birch"),
        heads("example/gone", "example/gone", "birch"),
        heads("example/kept-out", "example/kept-out", "birch"),
    ];

    let (read, queries) = read(vec![answer], &asks).await;

    let [own, forked, gone, fork_gone, missing, kept] = <[_; 6]>::try_from(read.answers).unwrap();
    let numbers = |found: &[super::super::heads::Headed]| found.iter().map(|pr| pr.number.get()).collect::<Vec<_>>();
    let (own, forked) = (own.unwrap(), forked.unwrap());
    assert_eq!(numbers(&own), [7], "its own, and no fork's");
    assert_eq!((own[0].state, own[0].head.clone()), (PrState::Merged, CommitId::from("head-7")));
    assert_eq!(own[0].url, "https://github.com/example/project/pull/7");
    assert_eq!(numbers(&forked), [8]);
    assert_eq!(forked[0].into, name("example/project"), "read where it merges into, rather than in the fork");
    assert_eq!(numbers(&gone.unwrap()), [5], "by its name, from its own repository");
    assert_eq!(numbers(&fork_gone.unwrap()), [] as [u64; 0], "a fork's branch that's gone has no head to be it");
    let unseen = |said: &str| HeadsUnread::Unseen { said: said.to_string() };
    assert_eq!(missing.unwrap_err(), unseen("Could not resolve to a Repository."), "unseen, rather than failed");
    assert_eq!(kept.unwrap_err(), unseen(kept_out));
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

    assert!(read.answers.iter().all(|answer| matches!(answer, Err(HeadsUnread::Failed(_)))), "{:?}", read.answers);
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
    let waits =
        |answer: &Result<_, HeadsUnread>| matches!(answer, Err(HeadsUnread::Failed(e)) if e.code == ErrorCode::Worker);
    assert!(read.answers[50..].iter().all(waits));
    assert!(queries[0].contains("r49: repository(") && !queries[0].contains("r50: repository("));
}
