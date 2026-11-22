//! What the hub asks GitHub for: each repository's numbers together, each once, and no more than a query carries.

use super::super::query::{ASKS_A_QUERY, grouped, query};
use super::parsing::{named, number};

/// #1021 of example/project, as one query asks for it, whole, so a change to what's asked is seen here.
#[test]
fn a_query_asks_for_an_issue_or_a_pull_request_by_its_number_and_whats_left_of_githubs_limit() {
    let asks = [(named("example/project"), number(1021))];

    let queries: Vec<String> = grouped(&asks).iter().map(query).collect();

    let shared = "number url title state createdAt author { login } comments { totalCount } bodyText \
        labels(first:20) { nodes { name color } }";
    assert_eq!(
        queries,
        [format!(
            "query {{ r0: repository(owner:\"example\",name:\"project\") {{ nameWithOwner \
             n1021: issueOrPullRequest(number:1021) {{ __typename ... on Issue {{ {shared} stateReason }} \
             ... on PullRequest {{ {shared} isDraft additions deletions reviewDecision mergeQueueEntry {{ state }} \
             mergeStateStatus mergeable baseRefName baseRefOid headRefOid \
             commits(last:1) {{ nodes {{ commit {{ statusCheckRollup {{ state }} }} }} }} }} }} }} \
             rateLimit {{ limit remaining resetAt }} }}"
        )]
    );
}

/// Two repositories' numbers, one of them asked twice, go in one query, each repository under its own alias in the
/// order it was first asked, with its numbers in it.
#[test]
fn asks_go_together_by_repository_and_each_is_asked_once() {
    let (project, tools) = (named("example/project"), named("ada/tools"));
    let asks = [
        (project.clone(), number(1021)),
        (tools.clone(), number(3)),
        (project.clone(), number(7)),
        (project.clone(), number(1021)),
    ];

    let grouped = grouped(&asks);

    assert_eq!(grouped, [vec![(&project, vec![number(1021), number(7)]), (&tools, vec![number(3)])]]);
    let query = query(&grouped[0]);
    assert!(query.contains(r#"r1: repository(owner:"ada",name:"tools") { nameWithOwner n3: issueOrPullRequest("#));
    assert_eq!(query.matches("n1021: ").count(), 1, "{query}");
}

/// A hundred and twenty asks of one repository, in queries of no more than a query carries.
#[test]
fn asks_past_what_a_query_carries_go_in_the_next() {
    let asks: Vec<_> = (1..=120).map(|n| (named("example/project"), number(n))).collect();

    let grouped = grouped(&asks);

    let sizes: Vec<usize> = grouped.iter().map(|batch| batch.iter().map(|(_, numbers)| numbers.len()).sum()).collect();
    assert_eq!(ASKS_A_QUERY, 50);
    assert_eq!(sizes, [50, 50, 20]);
    assert_eq!(grouped[1][0].1[0], number(51));
}
