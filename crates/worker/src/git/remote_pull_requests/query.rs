//! Queries are kept beside the reader. Every connection is paged before an observation can be used.

use protocol::model::PullRequestNumber;
use serde_json::Value;

pub(super) fn quote(value: &str) -> String {
    Value::String(value.to_owned()).to_string()
}

pub(super) fn cursor(after: Option<&str>) -> String {
    after.map_or_else(String::new, |after| format!(",after:{}", quote(after)))
}

pub(super) fn metadata(number: PullRequestNumber, after: Option<&str>) -> String {
    format!("pullRequest(number:{}) {{ {} {} }}", number.get(), FIELDS, stack(after))
}

fn stack(after: Option<&str>) -> String {
    format!(
        "stackEntry {{ position }} stack {{ baseRefName entries(first:100{}) {{ totalCount nodes {{ position pullRequest {{ number state }} }} pageInfo {{ hasNextPage endCursor }} }} }}",
        cursor(after)
    )
}

pub(super) const FIELDS: &str = "number url title state isDraft baseRefName baseRefOid headRefOid headRefName \
    mergeStateStatus mergeable reviewDecision potentialMergeCommit { oid } mergeQueueEntry { state } \
    reviews(states:[CHANGES_REQUESTED],last:1) { nodes { submittedAt } } \
    timelineItems(itemTypes:[REMOVED_FROM_MERGE_QUEUE_EVENT],last:1) { nodes { \
    ... on RemovedFromMergeQueueEvent { createdAt reason } } }";

pub(super) fn observation(number: PullRequestNumber, head: &str, after: Option<&str>) -> String {
    format!(
        "pullRequest(number:{n}) {{ {FIELDS} baseRef {{ target {{ oid }} compare(headRef:{head}) {{ behindBy baseTarget {{ oid }} headTarget {{ oid }} }} }} \
        statusCheckRollup {{ commit {{ oid }} contexts(first:100{after}) {{ nodes {{ __typename \
        ... on CheckRun {{ id name url status conclusion startedAt isRequired(pullRequestNumber:{n}) checkSuite {{ createdAt commit {{ oid }} app {{ databaseId }} workflowRun {{ workflow {{ id }} }} }} }} \
        ... on StatusContext {{ id context targetUrl state createdAt isRequired(pullRequestNumber:{n}) commit {{ oid }} }} \
        }} pageInfo {{ hasNextPage endCursor }} }} }} }}",
        n = number.get(),
        head = quote(head),
        after = cursor(after)
    )
}

pub(super) fn requirements(branch: &str, after: Option<&str>) -> String {
    format!(
        "ref(qualifiedName:{branch}) {{ refUpdateRule {{ requiredStatusCheckContexts }} branchProtectionRule {{ requiresStatusChecks requiredStatusChecks {{ context app {{ databaseId }} }} }} \
        rules(first:100{after}) {{ nodes {{ type repositoryRuleset {{ enforcement }} parameters {{ \
        ... on RequiredStatusChecksParameters {{ requiredStatusChecks {{ context integrationId }} }} }} }} pageInfo {{ hasNextPage endCursor }} }} }}",
        branch = quote(&format!("refs/heads/{branch}")),
        after = cursor(after)
    )
}
