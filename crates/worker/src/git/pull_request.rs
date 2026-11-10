//! The pull request for a worktree's branch, from `gh`, and how the worktree stands against it.

use std::collections::HashMap;
use std::path::Path;
use std::process::{Output, Stdio};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{Checks, Local, PrState, PullRequest, QueueState};
use protocol::worker::RateLimit;
use serde_json::Value;
use tokio::process::Command;

use super::{GH_TIMEOUT, git, ref_exists, status};

#[cfg(test)]
mod tests;

const FIELDS: &str =
    "number,url,title,state,isDraft,baseRefName,headRefOid,mergeStateStatus,reviewDecision,statusCheckRollup";
/// `gh pr view` can't say where a pull request is in the merge queue, so it's asked for by the pull request's URL.
const QUEUE_QUERY: &str =
    "query($url: URI!) { resource(url: $url) { ... on PullRequest { mergeQueueEntry { state } } } }";

/// The pull request for the current branch, or `None` when `gh` says it has none. An error means `gh` didn't say, so
/// whoever asked can keep what it knew rather than take it as none: [`ErrorCode::Unavailable`] when `gh` can't work
/// here at all, such as when it isn't installed or logged in, and [`ErrorCode::Worker`] when it couldn't this time,
/// such as while GitHub can't be reached. Where it is in the merge queue counts as part of the answer, since a failure
/// there would read as having left the queue. What the worktree can't say of itself is left at nothing to report.
pub async fn pull_request(path: &str) -> Result<Option<PullRequest>, ApiError> {
    let viewed = match run_gh(path, &["pr", "view", "--json", FIELDS]).await {
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
    let (mut pr, head) =
        parse_pull_request(&viewed.stdout).ok_or_else(|| couldnt_say("gh printed something other than one"))?;
    // Only an open pull request can still change, so only its queue and its worktree matter.
    if pr.state == PrState::Open {
        let queue = async { if pr.is_draft { Ok(None) } else { queue(path, &pr.url).await } };
        let (queue, local) = tokio::join!(queue, local(path, head.as_deref()));
        (pr.queue, pr.local) = (queue?, local);
    }
    Ok(Some(pr))
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

/// The pull request, with nothing yet about the queue or the worktree, and the commit at its head on GitHub.
fn parse_pull_request(json: &[u8]) -> Option<(PullRequest, Option<String>)> {
    let value: Value = serde_json::from_slice(json).ok()?;
    // GitHub's own words, which the protocol's types are named for. Anything else is unknown.
    let pr = PullRequest {
        number: value["number"].as_u64()?,
        url: value["url"].as_str()?.to_string(),
        title: value["title"].as_str().unwrap_or_default().to_string(),
        state: serde_json::from_value(value["state"].clone()).unwrap_or(PrState::Unknown),
        is_draft: value["isDraft"].as_bool().unwrap_or(false),
        base: value["baseRefName"].as_str().map(str::to_string),
        merge_state: serde_json::from_value(value["mergeStateStatus"].clone()).unwrap_or_default(),
        review: serde_json::from_value(value["reviewDecision"].clone()).unwrap_or_default(),
        checks: checks(&value["statusCheckRollup"]),
        queue: None,
        local: Local::default(),
    };
    Some((pr, value["headRefOid"].as_str().map(str::to_string)))
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

async fn queue(path: &str, url: &str) -> Result<Option<QueueState>, ApiError> {
    let (query, url) = (format!("query={QUEUE_QUERY}"), format!("url={url}"));
    let answer = gh(path, &["api", "graphql", "-f", &query, "-f", &url]).await;
    Ok(parse_queue(&answer.ok_or_else(|| couldnt_say("its place in the merge queue couldn't be read"))?))
}

/// `None` when it isn't in a queue, which GitHub answers with no entry.
fn parse_queue(json: &[u8]) -> Option<QueueState> {
    let value: Value = serde_json::from_slice(json).ok()?;
    serde_json::from_value(value["data"]["resource"]["mergeQueueEntry"]["state"].clone()).ok()
}

/// Against the pull request's head where the worktree has that commit, which it does once it's pushed from here or
/// fetched. Otherwise against the branch's upstream, as of the last fetch.
async fn local(path: &str, head: Option<&str>) -> Local {
    let Ok(status) = status(path).await else { return Local::default() };
    let (worktree, upstream) = (Path::new(path), (status.ahead, status.behind));
    let (ahead, behind) = match head {
        // A head that isn't here is at least one commit that isn't, such as one GitHub's Update branch or someone else
        // pushed. How many can't be told without fetching, so the upstream says, if it's been fetched since.
        Some(head) if !ref_exists(worktree, head).await.unwrap_or(true) => (upstream.0, upstream.1.max(1)),
        Some(head) => apart(worktree, head).await.unwrap_or(upstream),
        None => upstream,
    };
    Local { uncommitted: u32::try_from(status.files.len()).unwrap_or(u32::MAX), ahead, behind }
}

/// How many commits `HEAD` has that `head` doesn't, and the other way.
async fn apart(worktree: &Path, head: &str) -> Option<(u32, u32)> {
    let range = format!("HEAD...{head}");
    let output = git(worktree, &["rev-list", "--left-right", "--count", &range]).await.ok()?;
    if !output.success {
        return None;
    }
    let mut counts = output.stdout.split_whitespace().map(str::parse);
    Some((counts.next()?.ok()?, counts.next()?.ok()?))
}
