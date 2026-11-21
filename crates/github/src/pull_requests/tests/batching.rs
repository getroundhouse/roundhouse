//! Many asks read together: in the same queries, each to its own answer, and within GitHub's limits.

use super::super::query::Part;
use super::super::rounds::{PARTS_A_QUERY, batches};
use super::reader::{answer, answer_with, ask, pull_request, read, repository, rules};
use super::*;
use crate::Answer;

/// #3 of example/project, alone in its stack as far as a read of it goes, with its summary, and example/gone, which
/// isn't there.
#[tokio::test]
async fn asks_read_together_share_their_queries_and_one_that_fails_fails_no_other() {
    let gone = json!({"type":"NOT_FOUND","path":["r1"],"message":"Could not resolve to a Repository."});
    let (_, project) = repository(0, "project", &[("p3", pull_request(3))]);
    let first = answer_with(json!({"r0": project, "r1": null}), 4900, &[gone]);
    let second = answer(vec![repository(0, "project", &[("b0", rules()), ("v3", pull_request(3))])]);

    let (read, queries) = read(vec![first, second], vec![ask("project", &[3]), ask("gone", &[5])]).await;

    let [project, gone] = <[_; 2]>::try_from(read.answers).unwrap();
    let project = project.unwrap();
    assert_eq!(project.pull_requests.iter().map(|row| row.pull_request.number).collect::<Vec<_>>(), [3]);
    let gone = gone.unwrap_err();
    assert_eq!((gone.error.code, gone.affected), (ErrorCode::Unavailable, Vec::new()));
    let summaries: Vec<_> = read
        .summaries
        .iter()
        .map(|read| (read.number.get(), read.summary.as_ref().map(|summary| summary.comments)))
        .collect();
    assert_eq!(summaries, [(3, Ok(2))], "#3's summary, and none of the repository that isn't there");
    assert_eq!(queries.len(), 2);
    assert!(queries[0].contains("r0: repository(") && queries[0].contains("r1: repository("), "{}", queries[0]);
    assert!(!queries[1].contains("r1: repository("), "{}", queries[1]);
}

/// An error about the query rather than a part of it, as from a GitHub whose schema has no stacks, fails every ask
/// in it, as what reading again won't change.
#[tokio::test]
async fn an_error_about_the_whole_query_fails_every_ask_in_it() {
    let refused = json!({"type":"undefinedField","message":"Field 'stack' doesn't exist on type 'PullRequest'"});
    let answers =
        vec![Answer::With { status: 200, headers: Vec::new(), body: json!({ "errors": [refused] }).to_string() }];

    let (read, _) = read(answers, vec![ask("project", &[3]), ask("other", &[5])]).await;

    let codes: Vec<_> = read.answers.into_iter().map(|answer| answer.unwrap_err().error.code).collect();
    assert_eq!(codes, [ErrorCode::Unsupported, ErrorCode::Unsupported]);
}

/// GitHub's limit down to a fifth, which is left for agents and the person, once the first round has said so.
#[tokio::test]
async fn a_read_stops_asking_once_githubs_limit_is_down_to_what_is_left_for_others() {
    let first =
        answer_with(json!({"r0":{"nameWithOwner":"example/project","url":"u","p3":pull_request(3)}}), 1000, &[]);

    let (read, queries) = read(vec![first], vec![ask("project", &[3])]).await;

    let unread = read.answers.into_iter().next().unwrap().unwrap_err();
    assert_eq!(unread.error.code, ErrorCode::Worker);
    assert_eq!((queries.len(), read.rate_limit.unwrap().remaining), (1, 1000));
}

/// Seventy parts of one ask and fifty of another, each query no more than it carries, and each ask's parts in order.
#[test]
fn parts_go_into_queries_of_no_more_than_a_query_carries_an_ask_split_where_it_doesnt_fit() {
    let parts = |count: u64| (1..=count).map(|n| Part::Look(PullRequestNumber::try_from(n).unwrap())).collect();

    let batches = batches(vec![(0, parts(70)), (1, parts(50))]);

    let shapes: Vec<Vec<(usize, usize)>> =
        batches.iter().map(|batch| batch.iter().map(|(at, parts)| (*at, parts.len())).collect()).collect();
    assert_eq!(PARTS_A_QUERY, 50);
    assert_eq!(shapes, [vec![(0, 50)], vec![(0, 20), (1, 30)], vec![(1, 20)]]);
    assert_eq!(batches[1][0].1[0], Part::Look(PullRequestNumber::try_from(51).unwrap()));
}
