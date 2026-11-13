//! The pull request of the branch a worktree has checked out, from `gh`, and how the worktree stands against it.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Output, Stdio};

use chrono::DateTime;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{BranchName, Checks, CommitId, LeftQueue, Local, PrState, PullRequest, QueueState};
use protocol::worker::RateLimit;
use serde_json::Value;
use tokio::process::Command;

use self::link::Choice;
use self::local::local;
use super::GH_TIMEOUT;
use super::branches::checked_out;

mod link;
mod local;
#[cfg(test)]
mod tests;

/// `commits` is the first hundred, oldest first, which [`link`] goes by, with `headRefOid`.
const FIELDS: &str = "number,url,title,state,isDraft,baseRefName,headRefName,headRefOid,commits,mergeStateStatus,\
    reviewDecision,statusCheckRollup";
/// What `gh pr view` can't say, asked for by the pull request's URL: where it is in the merge queue, when it last left
/// it, and when a review last asked for changes. Its state comes too, since it can merge between the two calls, and
/// would otherwise read as having left the queue without merging. GitHub records a removal from the queue as the queue
/// merges one too, with the reason `merged`, a moment before it calls the pull request merged.
const BY_URL: &str = "query($url: URI!) { resource(url: $url) { ... on PullRequest { state mergeQueueEntry { state } \
    timelineItems(itemTypes: [REMOVED_FROM_MERGE_QUEUE_EVENT], last: 1) { nodes { ... on RemovedFromMergeQueueEvent { \
    createdAt reason } } } reviews(states: [CHANGES_REQUESTED], last: 1) { nodes { submittedAt } } } } }";

/// The pull request of the branch checked out, or `None` when it has none. An error means `gh` or git didn't say, so
/// whoever asked can keep what it knew rather than take it as none: [`ErrorCode::Unavailable`] when `gh` can't work
/// here at all, such as when it isn't installed or logged in, and [`ErrorCode::Worker`] when it couldn't this time,
/// such as while GitHub can't be reached. Where it is in the merge queue counts as part of the answer, since a failure
/// there would read as being in none. What the worktree can't say of itself is left at nothing to report.
pub async fn pull_request(path: &str, branch: Option<&BranchName>) -> Result<Option<PullRequest>, ApiError> {
    let Some(Viewed { mut pull_request, head, .. }) = of_branch(path, branch).await? else { return Ok(None) };
    // Only an open pull request can still change, so only what the second call says and its worktree matter.
    if pull_request.state == PrState::Open {
        let head = head.as_ref().map(CommitId::as_str);
        let (by_url, local) = tokio::join!(by_url(path, &pull_request.url), local(path, head));
        by_url?.fill(&mut pull_request);
        pull_request.local = local;
    }
    Ok(Some(pull_request))
}

/// The one `gh` finds by the branch's name, where it's the branch's, as [`link`] says, or else the one the branch is
/// linked to, read by its URL. `gh` asks GitHub again for that only when it finds another by the name, or none, as
/// for a branch renamed since it was pushed, which the hub's count of a read's calls leaves out, and GitHub's own
/// count of what's left, which the hub asks for, takes in.
async fn of_branch(path: &str, branch: Option<&BranchName>) -> Result<Option<Viewed>, ApiError> {
    let worktree = Path::new(path);
    let by_name = view(path, None).await?;
    // `gh` looks for the one checked out as it runs, so another checked out since `branch` was read would be judged
    // by that one's history and linked to it. The next read, which switching branch brings, says.
    if checked_out(worktree).await.as_ref() != branch {
        return Err(couldnt_say("another branch was checked out while it was read"));
    }
    let Some(branch) = branch else { return Ok(None) };
    let linked = link::linked(worktree, branch).await?;
    let has_been_at = match &by_name {
        Some(found) if linked.as_deref() != Some(found.pull_request.url.as_str()) => {
            link::has_been_at(worktree, branch, &found.commits(), found.from.as_ref()).await?
        }
        _ => false,
    };
    let found_url = by_name.as_ref().map(|found| found.pull_request.url.as_str());
    match link::choose(found_url, linked.as_deref(), has_been_at) {
        Choice::Found => Ok(by_name),
        Choice::FoundAndLink => {
            if let Some(url) = found_url {
                link::link(worktree, branch, url).await;
            }
            Ok(by_name)
        }
        Choice::ReadLinked => match linked {
            Some(url) => view(path, Some(&url)).await,
            None => Ok(None),
        },
        Choice::None => Ok(None),
    }
}

/// A pull request as `gh pr view` prints it, with nothing yet about the queue or the worktree, and the commits of the
/// branch it's from, as GitHub has them.
#[derive(Debug, PartialEq)]
struct Viewed {
    pull_request: PullRequest,
    /// The branch it's from, by its name there.
    from: Option<BranchName>,
    /// The newest.
    head: Option<CommitId>,
    /// The first hundred, oldest first.
    first: Vec<CommitId>,
}

impl Viewed {
    /// Every one it says, the newest included, which a pull request of more than a hundred has after the first.
    fn commits(&self) -> Vec<CommitId> {
        let newest = self.head.iter().filter(|head| !self.first.contains(head));
        self.first.iter().chain(newest).cloned().collect()
    }
}

/// The pull request at `url`, or the one `gh` finds for the branch checked out, by its name. `None` when `gh` says
/// there's none.
async fn view(path: &str, url: Option<&str>) -> Result<Option<Viewed>, ApiError> {
    let args: Vec<&str> = ["pr", "view"].into_iter().chain(url).chain(["--json", FIELDS]).collect();
    let viewed = match run_gh(path, &args).await {
        Ok(viewed) => viewed,
        Err(Unran::Missing) => return Err(cant_work_here("gh isn't installed")),
        Err(Unran::Failed(why)) => return Err(couldnt_say(&why)),
    };
    if !viewed.status.success() {
        let said = String::from_utf8_lossy(&viewed.stderr);
        return match failing(viewed.status.code(), &said) {
            Failing::NoneFound => Ok(None),
            Failing::CantWorkHere => Err(cant_work_here(said.trim())),
            Failing::CouldntSay => Err(couldnt_say(said.trim())),
        };
    }
    parse_pull_request(&viewed.stdout).map(Some).ok_or_else(|| couldnt_say("gh printed something other than one"))
}

/// What a failed `gh pr view` means.
#[derive(Debug, PartialEq, Eq)]
enum Failing {
    /// There's no pull request for the branch, which `gh` says with the same exit code as a failure.
    NoneFound,
    /// `gh` can't work here until something changes: it isn't logged in, or the repository isn't on GitHub.
    CantWorkHere,
    /// It might next time, such as once GitHub can be reached.
    CouldntSay,
}

/// From its exit code, which is 4 when it needs a login, and its own words, as version 2.103 says them.
fn failing(code: Option<i32>, stderr: &str) -> Failing {
    const NOT_ON_GITHUB: [&str; 2] = ["no git remotes found", "none of the git remotes configured for this repository"];
    if stderr.contains("no pull requests found") {
        Failing::NoneFound
    } else if code == Some(4) || NOT_ON_GITHUB.iter().any(|words| stderr.contains(words)) {
        Failing::CantWorkHere
    } else {
        Failing::CouldntSay
    }
}

/// How much of GitHub's hourly GraphQL limit is left, which is what `gh pr view` spends. GitHub's `rate_limit` endpoint
/// says, in the same numbers as the headers on every answer, and asking it doesn't count against the limit.
pub async fn rate_limit(path: &str) -> Result<RateLimit, ApiError> {
    let said = gh(path, &["api", "rate_limit"]).await.ok_or_else(|| couldnt_count("gh couldn't ask"))?;
    parse_rate_limit(&said).ok_or_else(|| couldnt_count("GitHub answered something else"))
}

fn parse_rate_limit(json: &[u8]) -> Option<RateLimit> {
    let value: Value = serde_json::from_slice(json).ok()?;
    let graphql = &value["resources"]["graphql"];
    let count = |field: &str| graphql[field].as_u64().and_then(|n| u32::try_from(n).ok());
    let reset_s = graphql["reset"].as_i64()?;
    Some(RateLimit { limit: count("limit")?, remaining: count("remaining")?, resets_at_ms: reset_s.checked_mul(1000)? })
}

fn couldnt_count(why: &str) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("couldn't tell how much of GitHub's limit is left: {why}"))
}

fn couldnt_say(why: &str) -> ApiError {
    ApiError::new(ErrorCode::Worker, format!("couldn't tell whether there's a pull request: {why}"))
}

fn cant_work_here(why: &str) -> ApiError {
    ApiError::new(ErrorCode::Unavailable, format!("gh can't say whether there's a pull request here: {why}"))
}

/// What it printed, if it succeeded.
async fn gh(path: &str, args: &[&str]) -> Option<Vec<u8>> {
    let output = run_gh(path, args).await.ok()?;
    output.status.success().then_some(output.stdout)
}

/// Why `gh` didn't run to the end.
enum Unran {
    Missing,
    /// It couldn't start for another reason, or took too long.
    Failed(String),
}

async fn run_gh(path: &str, args: &[&str]) -> Result<Output, Unran> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(path)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output();
    match tokio::time::timeout(GH_TIMEOUT, output).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => Err(Unran::Missing),
        Ok(Err(e)) => Err(Unran::Failed(e.to_string())),
        Err(_) => Err(Unran::Failed(format!("gh took more than {GH_TIMEOUT:?}"))),
    }
}

fn parse_pull_request(json: &[u8]) -> Option<Viewed> {
    let value: Value = serde_json::from_slice(json).ok()?;
    // GitHub's own words, which the protocol's types are named for. Anything else is unknown.
    let pull_request = PullRequest {
        number: value["number"].as_u64()?,
        url: value["url"].as_str()?.to_string(),
        title: value["title"].as_str().unwrap_or_default().to_string(),
        state: serde_json::from_value(value["state"].clone()).unwrap_or(PrState::Unknown),
        is_draft: value["isDraft"].as_bool().unwrap_or(false),
        base: value["baseRefName"].as_str().map(BranchName::from),
        merge_state: serde_json::from_value(value["mergeStateStatus"].clone()).unwrap_or_default(),
        review: serde_json::from_value(value["reviewDecision"].clone()).unwrap_or_default(),
        checks: checks(&value["statusCheckRollup"]),
        queue: None,
        left_queue: None,
        changes_requested_at_ms: None,
        local: Local::default(),
    };
    let first = value["commits"].as_array().into_iter().flatten();
    let first = first.filter_map(|commit| commit["oid"].as_str()).map(CommitId::from).collect();
    let from = value["headRefName"].as_str().map(BranchName::from);
    Some(Viewed { pull_request, from, head: value["headRefOid"].as_str().map(CommitId::from), first })
}

enum Outcome {
    Passed,
    Failed,
    Pending,
}

/// Each check once, as its latest run. GitHub keeps a check's earlier runs on the same commit, such as one cancelled
/// for a newer push or one run again, and counting those would say a pull request whose checks passed is failing.
fn checks(rollup: &Value) -> Checks {
    let mut latest: HashMap<(&str, &str), &Value> = HashMap::new();
    for check in rollup.as_array().into_iter().flatten() {
        let name = check["name"].as_str().or_else(|| check["context"].as_str()).unwrap_or_default();
        let key = (name, check["workflowName"].as_str().unwrap_or_default());
        if latest.get(&key).is_none_or(|seen| started(seen) <= started(check)) {
            latest.insert(key, check);
        }
    }
    let mut checks = Checks::default();
    for check in latest.into_values() {
        match outcome(check) {
            Outcome::Passed => checks.passed += 1,
            Outcome::Failed => checks.failed += 1,
            Outcome::Pending => checks.pending += 1,
        }
    }
    checks
}

/// When a run started, in GitHub's timestamps, which sort as text. One that hasn't started yet, which GitHub gives a
/// time in the year 1, comes after every one that has, since it's the newest.
fn started(check: &Value) -> (bool, &str) {
    match check["startedAt"].as_str().filter(|at| !at.starts_with("0001-")) {
        Some(at) => (false, at),
        None => (true, ""),
    }
}

/// A check run has a status, and a conclusion once it's completed. A commit status, from GitHub's older API, has
/// only a state. Anything finished that isn't known to have passed counts as failed, so nothing is called ready
/// that might not be.
fn outcome(check: &Value) -> Outcome {
    let word = |field: &str| check[field].as_str().unwrap_or_default();
    if word("__typename") == "StatusContext" {
        return match word("state") {
            "SUCCESS" => Outcome::Passed,
            "PENDING" | "EXPECTED" => Outcome::Pending,
            _ => Outcome::Failed,
        };
    }
    if word("status") != "COMPLETED" {
        return Outcome::Pending;
    }
    match word("conclusion") {
        "SUCCESS" | "NEUTRAL" | "SKIPPED" => Outcome::Passed,
        _ => Outcome::Failed,
    }
}

/// What the second call says of an open pull request.
#[derive(Debug, PartialEq, Eq)]
struct ByUrl {
    /// Its state as of this call, which is later than `gh pr view`'s. `None` when GitHub didn't say.
    state: Option<PrState>,
    /// Where it is in the merge queue, or `None` when it isn't in one.
    queue: Option<QueueState>,
    left_queue: Option<LeftQueue>,
    changes_requested_at_ms: Option<i64>,
}

impl ByUrl {
    fn fill(self, pr: &mut PullRequest) {
        pr.state = self.state.unwrap_or(pr.state);
        (pr.queue, pr.left_queue, pr.changes_requested_at_ms) =
            (self.queue, self.left_queue, self.changes_requested_at_ms);
    }
}

async fn by_url(path: &str, url: &str) -> Result<ByUrl, ApiError> {
    let (query, url) = (format!("query={BY_URL}"), format!("url={url}"));
    let answer = gh(path, &["api", "graphql", "-f", &query, "-f", &url]).await;
    let unread = || couldnt_say("its place in the merge queue couldn't be read");
    parse_by_url(&answer.ok_or_else(unread)?).ok_or_else(unread)
}

/// `None` when GitHub answered something other than the pull request, which can't be taken as being in no queue.
fn parse_by_url(json: &[u8]) -> Option<ByUrl> {
    let value: Value = serde_json::from_slice(json).ok()?;
    let pr = value["data"]["resource"].as_object()?;
    let entry = pr.get("mergeQueueEntry")?;
    let queue =
        (!entry.is_null()).then(|| serde_json::from_value(entry["state"].clone()).unwrap_or(QueueState::Unknown));
    let mut state = pr.get("state").and_then(|state| serde_json::from_value(state.clone()).ok());
    let newest = |field: &str| pr.get(field).and_then(|items| items["nodes"].as_array()).and_then(|nodes| nodes.last());
    let mut left_queue = None;
    if let Some(removal) = newest("timelineItems") {
        match removal["reason"].as_str() {
            Some("merged") => state = Some(PrState::Merged),
            reason => {
                let reason = reason.unwrap_or_default().to_string();
                left_queue = at_ms(&removal["createdAt"]).map(|at_ms| LeftQueue { at_ms, reason });
            }
        }
    }
    let changes_requested_at_ms = newest("reviews").and_then(|review| at_ms(&review["submittedAt"]));
    Some(ByUrl { state, queue, left_queue, changes_requested_at_ms })
}

/// One of GitHub's times, in milliseconds since the epoch.
fn at_ms(at: &Value) -> Option<i64> {
    DateTime::parse_from_rfc3339(at.as_str()?).ok().map(|at| at.timestamp_millis())
}
