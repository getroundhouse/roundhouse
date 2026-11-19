//! The GraphQL the hub reads pull requests with, as pieces that go together into one query: each repository asked
//! of under an alias of its own, and in it each pull request, page or branch under an alias that says which, so one
//! query can carry many asks, and each part of the answer finds its way back by its alias.

use protocol::model::{GithubRepositoryName, PullRequestNumber};
use serde_json::Value;

/// A pull request's own fields: what it is, where it stands, and what the merge queue and reviews last recorded.
pub(super) const FIELDS: &str = "number url title state isDraft baseRefName baseRefOid headRefOid headRefName \
    mergeStateStatus mergeable reviewDecision potentialMergeCommit { oid } mergeQueueEntry { state } \
    reviews(states:[CHANGES_REQUESTED],last:1) { nodes { submittedAt } } \
    timelineItems(itemTypes:[REMOVED_FROM_MERGE_QUEUE_EVENT],last:1) { nodes { \
    ... on RemovedFromMergeQueueEvent { createdAt reason } } }";

/// What one part of a query asks of a repository, under the alias [`Part::alias`] gives it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Part {
    /// A pull request's fields, its place in its stack and the first page of it, and the first page of its checks.
    Look(PullRequestNumber),
    /// A page past the first of a pull request's stack.
    Stack(PullRequestNumber, String),
    /// A page past the first of a pull request's checks.
    Checks { number: PullRequestNumber, after: String },
    /// A branch's rules for checks, the `index`th the ask has, from the page `after`.
    Branch { index: usize, branch: String, after: Option<String> },
    /// A pull request's fields and the first page of its stack again, to confirm none moved while it was read, and
    /// for one with lower pull requests, how far its head is behind its base's.
    Confirm { number: PullRequestNumber, head: Option<String> },
}

impl Part {
    /// The pull request it's about, if it's about one.
    pub(super) fn about(&self) -> Option<PullRequestNumber> {
        match self {
            Part::Look(number)
            | Part::Stack(number, _)
            | Part::Checks { number, .. }
            | Part::Confirm { number, .. } => Some(*number),
            Part::Branch { .. } => None,
        }
    }

    /// Its alias in its repository: a letter for the kind of part, then the pull request's number or the branch's
    /// index, as GraphQL's names allow.
    pub(super) fn alias(&self) -> String {
        match self {
            Part::Look(number) => format!("p{}", number.get()),
            Part::Stack(number, _) => format!("s{}", number.get()),
            Part::Checks { number, .. } => format!("c{}", number.get()),
            Part::Branch { index, .. } => format!("b{index}"),
            Part::Confirm { number, .. } => format!("v{}", number.get()),
        }
    }

    fn written(&self) -> String {
        let alias = self.alias();
        match self {
            Part::Look(number) => {
                let rollup = rollup(*number, None);
                format!("{alias}: pullRequest(number:{}) {{ {FIELDS} {} {rollup} }}", number.get(), stack(None))
            }
            Part::Stack(number, after) => {
                format!("{alias}: pullRequest(number:{}) {{ headRefOid {} }}", number.get(), stack(Some(after)))
            }
            Part::Checks { number, after, .. } => {
                format!(
                    "{alias}: pullRequest(number:{}) {{ headRefOid potentialMergeCommit {{ oid }} {} }}",
                    number.get(),
                    rollup(*number, Some(after))
                )
            }
            Part::Branch { branch, after, .. } => requirements(&alias, branch, after.as_deref()),
            Part::Confirm { number, head } => {
                let compare = head.as_ref().map_or_else(String::new, |head| {
                    format!(
                        "baseRef {{ compare(headRef:{}) {{ behindBy baseTarget {{ oid }} headTarget {{ oid }} }} }}",
                        quote(head)
                    )
                });
                format!("{alias}: pullRequest(number:{}) {{ {FIELDS} {} {compare} }}", number.get(), stack(None))
            }
        }
    }
}

/// The parts asked of one repository, under its alias, `r` and the ask's index among the query's.
pub(super) struct Asked<'a> {
    pub(super) index: usize,
    pub(super) repository: &'a GithubRepositoryName,
    pub(super) parts: Vec<Part>,
}

/// One query asking each of `asked`, with what it costs and what's left of GitHub's limit.
pub(super) fn query(asked: &[Asked<'_>]) -> String {
    let repositories: Vec<String> = asked
        .iter()
        .map(|asked| {
            let parts: Vec<String> = asked.parts.iter().map(Part::written).collect();
            format!(
                "r{}: repository({}) {{ nameWithOwner url {} }}",
                asked.index,
                named(asked.repository),
                parts.join(" ")
            )
        })
        .collect();
    format!("query {{ {} rateLimit {{ limit remaining resetAt }} }}", repositories.join(" "))
}

/// `repository`'s owner and name, as GraphQL's `repository` takes them.
pub(crate) fn named(repository: &GithubRepositoryName) -> String {
    let name = repository.as_str().split_once('/').map_or("", |(_, name)| name);
    format!("owner:{},name:{}", quote(repository.owner()), quote(name))
}

pub(super) fn quote(value: &str) -> String {
    Value::String(value.to_owned()).to_string()
}

fn cursor(after: Option<&str>) -> String {
    after.map_or_else(String::new, |after| format!(",after:{}", quote(after)))
}

fn stack(after: Option<&str>) -> String {
    format!(
        "stackEntry {{ position }} stack {{ baseRefName entries(first:100{}) {{ totalCount nodes {{ position \
        pullRequest {{ number state }} }} pageInfo {{ hasNextPage endCursor }} }} }}",
        cursor(after)
    )
}

fn rollup(number: PullRequestNumber, after: Option<&str>) -> String {
    format!(
        "statusCheckRollup {{ commit {{ oid }} contexts(first:100{after}) {{ nodes {{ __typename \
        ... on CheckRun {{ id name url status conclusion startedAt isRequired(pullRequestNumber:{n}) checkSuite {{ \
        createdAt commit {{ oid }} app {{ databaseId }} workflowRun {{ workflow {{ id }} }} }} }} \
        ... on StatusContext {{ id context targetUrl state createdAt isRequired(pullRequestNumber:{n}) commit {{ oid }} }} \
        }} pageInfo {{ hasNextPage endCursor }} }} }}",
        n = number.get(),
        after = cursor(after)
    )
}

fn requirements(alias: &str, branch: &str, after: Option<&str>) -> String {
    format!(
        "{alias}: ref(qualifiedName:{branch}) {{ refUpdateRule {{ requiredStatusCheckContexts }} branchProtectionRule {{ \
        requiresStatusChecks requiredStatusChecks {{ context app {{ databaseId }} }} }} \
        rules(first:100{after}) {{ nodes {{ type repositoryRuleset {{ enforcement }} parameters {{ \
        ... on RequiredStatusChecksParameters {{ requiredStatusChecks {{ context integrationId }} }} }} }} \
        pageInfo {{ hasNextPage endCursor }} }} }}",
        branch = quote(&format!("refs/heads/{branch}")),
        after = cursor(after)
    )
}
