//! `get_pull_request`: where the workspace's pull request stands, as the hub reads it from GitHub and works it out.
//! It's for the agent, which reads it as words, so it says in full what the app's pull request bar says in a few, and
//! says the things an agent gets wrong, such as a queued pull request not being merged yet.

use protocol::model::{Checks, MergeState, PullRequest, QueueState, Review, SessionId, Standing};
use protocol::worker::{PullRequestAnswer, ReadPullRequest};
use serde_json::{Value, json};

use super::mcp::Called;
use crate::asks::Asks;

#[cfg(test)]
pub(super) mod tests;

pub(super) const NAME: &str = "get_pull_request";

const DESCRIPTION: &str = "Says where the pull request of the branch checked out in this workspace stands on \
    GitHub: open, a draft, merged or closed; whether it's in the merge queue; its checks on its newest commit; its \
    review; whether it conflicts; and how this worktree differs from it. Roundhouse reads it from GitHub when asked, \
    waiting a few seconds when it read it just before, within its share of GitHub's hourly limit, and works out \
    what's in the way of merging it, so use this rather than `gh pr view`, `gh pr checks` or a loop of your own. A \
    pull request in the merge queue isn't merged until this says it's merged.";

pub(super) fn definition() -> Value {
    json!({
        "name": NAME,
        "title": "Get the pull request",
        "description": DESCRIPTION,
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
        "annotations": { "readOnlyHint": true, "openWorldHint": true },
    })
}

pub(super) async fn call(asks: &Asks, session_id: &SessionId) -> Called {
    match asks.ask(session_id.clone(), ReadPullRequest).await {
        Ok(PullRequestAnswer { pull_request }) => Called::text(describe(pull_request.as_ref())),
        Err(e) => Called::failed(format!("Roundhouse couldn't say where the pull request stands: {}", e.message)),
    }
}

/// The pull request in words: what it is, where it stands, then what GitHub says of it and what's here that isn't.
pub(super) fn describe(pull_request: Option<&PullRequest>) -> String {
    let Some(pr) = pull_request else {
        return "This workspace's branch has no pull request on GitHub.".to_string();
    };
    let into = pr.base.as_ref().map_or_default(|base| format!(", into {base}"));
    let stands = Standing::of(Some(pr));
    let mut lines = vec![format!("Pull request #{}, \"{}\"{into}: {}", pr.number, pr.title, pr.url)];
    lines.push(standing(stands));
    if matches!(stands, Standing::Merged | Standing::Closed) {
        return lines.join("\n");
    }
    lines.push(format!("Checks on its newest commit: {}.", checks(pr.checks)));
    lines.push(format!("Review: {}.", review(pr.review)));
    lines.push(format!("GitHub's merge state: {}.", merge_state(pr.merge_state)));
    let local = pr.local;
    lines.push(format!(
        "This worktree: {} changed and not committed, {} not pushed, and {} on the pull request that aren't here.",
        count(local.uncommitted, "file", "files"),
        count(local.ahead, "commit", "commits"),
        count(local.behind, "commit", "commits"),
    ));
    lines.join("\n")
}

/// The one thing most in the way of merging it, or that nothing is.
pub(super) fn standing(standing: Standing) -> String {
    match standing {
        Standing::NoPullRequest => "This workspace's branch has no pull request on GitHub.".to_string(),
        Standing::Merged => "It's merged.".to_string(),
        Standing::Closed => "It's closed, without being merged.".to_string(),
        Standing::Queued(queue) => queued(queue).to_string(),
        Standing::Uncommitted(files) => format!(
            "This worktree has {} changed and not committed, so what GitHub says is about an older commit.",
            count(files, "file", "files")
        ),
        Standing::Diverged => "This worktree and the pull request each have commits the other doesn't.".to_string(),
        Standing::Behind(commits) => {
            format!("The pull request has {} that this worktree doesn't.", count(commits, "commit", "commits"))
        }
        Standing::Unpushed(commits) => format!(
            "{} here {} on the pull request yet, so what GitHub says is about an older commit.",
            count(commits, "commit", "commits"),
            if commits == 1 { "isn't" } else { "aren't" }
        ),
        Standing::Conflicts => {
            "It conflicts with its base branch, so it can't merge until that's resolved.".to_string()
        }
        Standing::ChangesRequested => "A reviewer has asked for changes.".to_string(),
        Standing::Draft => "It's a draft, so it can't merge until it's marked ready.".to_string(),
        Standing::ChecksFailed(checks) => {
            format!("{} failed, so it can't merge as it is.", count(checks.failed, "check", "checks"))
        }
        Standing::ChecksRunning(checks) => format!(
            "Its checks are still running: {} of {} are done.",
            checks.passed + checks.failed,
            checks.passed + checks.failed + checks.pending
        ),
        Standing::ReviewRequired => "It needs a review before it can merge.".to_string(),
        Standing::Blocked => {
            "The repository's rules hold it back, such as needing it up to date with its base branch.".to_string()
        }
        Standing::Checking => "GitHub is still working out whether it can merge, which takes a moment.".to_string(),
        Standing::Ready => "It's ready to merge.".to_string(),
        Standing::Unknown => {
            "GitHub said something about it that Roundhouse can't read, so look at the pull request itself.".to_string()
        }
    }
}

fn queued(queue: QueueState) -> &'static str {
    match queue {
        QueueState::AwaitingChecks => {
            "It's in the merge queue, waiting on the checks the queue runs, so it isn't merged yet."
        }
        QueueState::Locked => "It's in the merge queue, which is held for now, so it isn't merged yet.",
        QueueState::Mergeable => {
            "It's in the merge queue and has passed, so it merges when its turn comes. It isn't \
            merged yet."
        }
        QueueState::Queued => "It's in the merge queue, waiting its turn, so it isn't merged yet.",
        QueueState::Unmergeable => "It's in the merge queue but can't merge, so it's leaving the queue.",
        QueueState::Unknown => "It's in the merge queue, so it isn't merged yet.",
    }
}

fn checks(checks: Checks) -> String {
    if checks == Checks::default() {
        return "none".to_string();
    }
    format!("{} passed, {} failed, {} running", checks.passed, checks.failed, checks.pending)
}

fn review(review: Review) -> &'static str {
    match review {
        Review::Approved => "approved",
        Review::ChangesRequested => "changes requested",
        Review::Required => "needed, and not given yet",
        Review::NotRequired => "not needed",
        Review::Unknown => "something Roundhouse can't read",
    }
}

/// The word GitHub used, which is what an agent that knows GitHub knows it by, or that it said one Roundhouse doesn't
/// know.
fn merge_state(state: MergeState) -> &'static str {
    match state {
        MergeState::Behind => "BEHIND",
        MergeState::Blocked => "BLOCKED",
        MergeState::Clean => "CLEAN",
        MergeState::Dirty => "DIRTY",
        MergeState::Draft => "DRAFT",
        MergeState::HasHooks => "HAS_HOOKS",
        MergeState::Unstable => "UNSTABLE",
        MergeState::Computing => "UNKNOWN, while it works it out",
        MergeState::Unknown => "a word Roundhouse doesn't know",
    }
}

fn count(n: u32, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}
