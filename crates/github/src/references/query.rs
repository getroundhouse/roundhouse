//! The GraphQL the hub reads issues and pull requests by number with: each repository asked of under an alias of its
//! own, `r` and its place in the query, and in it each number under `n` and the number, so one query carries many,
//! and each part of the answer, and each error GitHub gives, finds its way back by its aliases.

use std::collections::HashSet;

use protocol::model::{GithubRepositoryName, IssueNumber};

use crate::pull_requests::named;

/// The issues and pull requests one query asks for at most. Each asks for up to [`LABELS`] labels and one commit, so
/// a query of these costs GitHub's limit a point or so.
pub(super) const ASKS_A_QUERY: usize = 50;

/// The labels asked for of each, more than a chip or a card shows.
const LABELS: usize = 20;

/// What an issue and a pull request both have that's read: what it is, where it stands, who opened it and when, and
/// the start of what it says.
const SHARED: &str = "number url title state createdAt author { login } comments { totalCount } bodyText";

/// The repositories one query asks of, each with the numbers asked of it, in the order their aliases number them.
pub(super) type Grouped<'a> = Vec<(&'a GithubRepositoryName, Vec<IssueNumber>)>;

/// `asks` in queries of at most [`ASKS_A_QUERY`], each asked once however many times it's asked, and in each query
/// grouped by repository, in the order each was first asked.
pub(super) fn grouped(asks: &[(GithubRepositoryName, IssueNumber)]) -> Vec<Grouped<'_>> {
    let mut seen = HashSet::new();
    let distinct: Vec<(&GithubRepositoryName, IssueNumber)> =
        asks.iter().map(|(repository, number)| (repository, *number)).filter(|ask| seen.insert(*ask)).collect();
    distinct
        .chunks(ASKS_A_QUERY)
        .map(|chunk| {
            let mut grouped: Grouped<'_> = Vec::new();
            for &(repository, number) in chunk {
                match grouped.iter_mut().find(|(asked, _)| *asked == repository) {
                    Some((_, numbers)) => numbers.push(number),
                    None => grouped.push((repository, vec![number])),
                }
            }
            grouped
        })
        .collect()
}

/// One query asking for each of `grouped`, and what's left of GitHub's limit.
pub(super) fn query(grouped: &Grouped<'_>) -> String {
    let repositories: Vec<String> = grouped
        .iter()
        .enumerate()
        .map(|(at, (repository, numbers))| {
            let asked: Vec<String> = numbers.iter().map(|number| asked(*number)).collect();
            format!("{}: repository({}) {{ nameWithOwner {} }}", alias(at), named(repository), asked.join(" "))
        })
        .collect();
    format!("query {{ {} rateLimit {{ limit remaining resetAt }} }}", repositories.join(" "))
}

/// The alias of the repository `at` its place in the query.
pub(super) fn alias(at: usize) -> String {
    format!("r{at}")
}

/// The alias of `number` in its repository.
pub(super) fn number_alias(number: IssueNumber) -> String {
    format!("n{number}")
}

/// `number` as an issue or a pull request, whichever it is, as GitHub numbers both together.
fn asked(number: IssueNumber) -> String {
    let labels = format!("labels(first:{LABELS}) {{ nodes {{ name color }} }}");
    format!(
        "{}: issueOrPullRequest(number:{number}) {{ __typename ... on Issue {{ {SHARED} {labels} stateReason }} \
         ... on PullRequest {{ {SHARED} {labels} isDraft additions deletions reviewDecision \
         commits(last:1) {{ nodes {{ commit {{ statusCheckRollup {{ state }} }} }} }} }} }}",
        number_alias(number)
    )
}
