//! `wait_for_pull_request`: a wait on the workspace's pull request, which the hub keeps, answered as an MCP task whose
//! ID is the wait's, so the agent's turn can end while it waits, and its CLI hears how it ended by asking `tasks/get`.
//! The task is only a copy of the hub's wait, so it outlasts the worker, the hub and the agent's process alike.

use chrono::{DateTime, SecondsFormat};
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{PullRequest, SessionId, Standing, ToolUseId, Wait, WaitEnd, WaitUntil};
use protocol::worker::{CancelWait, ReadWait, StartWait, WaitAnswer};
use serde_json::{Map, Value, json};

use super::mcp::{Called, Code, RpcError};
use super::pull_request;
use crate::asks::Asks;

#[cfg(test)]
pub(super) mod tests;

pub(super) const NAME: &str = protocol::worker::WAIT_TOOL;

/// How often the agent's CLI is asked to check on a wait. The hub reads the pull request less often than this, so it's
/// only how soon after the read that ends a wait the agent hears of it, at the cost of a question to the hub that never
/// reads GitHub. *Unconfirmed*
const POLL_INTERVAL_MS: u64 = 5_000;

/// Where the Claude CLI names the agent's call, in a call's `_meta`.
const CALL_KEY: &str = "claudecode/toolUseId";

const DESCRIPTION: &str = "Waits on the pull request of the branch checked out in this workspace until it's merged \
    or closed, or needs you: a check failed, it conflicts, a reviewer asked for changes, it left the merge queue, or \
    it's a draft. With `until` set to `checks`, it also ends once its checks have run and none failed, so you can then \
    queue it. Roundhouse does the waiting, reading the pull request from GitHub within its share of GitHub's hourly \
    limit, so this returns at once and waits in the background, and you hear how it ended, with where the pull \
    request stands, when it does. You can end your turn meanwhile. Use this rather than a loop of `gh pr checks`, \
    `sleep` or `get_pull_request`. You wait on one at a time, so starting another ends the last, and checking out \
    another branch ends it too. The checks it waits on are this pull request's own, not those of the ones below it \
    in a stack.";

pub(super) fn definition() -> Value {
    json!({
        "name": NAME,
        "title": "Wait on the pull request",
        "description": DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "until": {
                    "type": "string",
                    "enum": ["done", "checks"],
                    "description": "`done`, the default, waits until it's merged or closed, or needs you. `checks` \
                        also stops once its checks have run and none failed.",
                },
            },
            "additionalProperties": false,
        },
        "annotations": { "readOnlyHint": true, "openWorldHint": true },
    })
}

/// Starts a wait, answered as a task. One that ended as it started, such as on a pull request that's merged already,
/// is answered at once, and so is one that couldn't start, with why.
pub(super) async fn call(asks: &Asks, session_id: &SessionId, params: &Map<String, Value>) -> Value {
    let until = match until(params) {
        Ok(until) => until,
        Err(wrong) => return Called::failed(wrong).to_json(),
    };
    let call = params.get("_meta").and_then(|meta| meta.get(CALL_KEY)).and_then(Value::as_str).map(ToolUseId::from);
    match asks.ask(session_id.clone(), StartWait { until, call }).await {
        Ok(WaitAnswer { wait, pull_request }) => match wait.ended {
            None => {
                let mut started = task(&wait, pull_request.as_ref());
                started["resultType"] = "task".into();
                started
            }
            Some(ended) => Called::text(ended_text(&wait, ended.how, pull_request.as_ref())).to_json(),
        },
        Err(e) => Called::failed(format!("Roundhouse couldn't start the wait: {}", e.message)).to_json(),
    }
}

/// `tasks/get`: how the wait stands, with how it ended once it has.
pub(super) async fn get(asks: &Asks, session_id: &SessionId, task_id: &str) -> Result<Value, RpcError> {
    let asked = asks.ask(session_id.clone(), ReadWait { wait: task_id.into() }).await;
    let WaitAnswer { wait, pull_request } = asked.map_err(|e| not_said(&e))?;
    Ok(task(&wait, pull_request.as_ref()))
}

/// `tasks/cancel`: ends the wait, unless it has ended already.
pub(super) async fn cancel(asks: &Asks, session_id: &SessionId, task_id: &str) -> Result<Value, RpcError> {
    asks.ask(session_id.clone(), CancelWait { wait: task_id.into() }).await.map_err(|e| not_said(&e))?;
    Ok(json!({ "resultType": "complete" }))
}

/// What `until` says, which the agent wrote, so a wrong one is the tool's failure, for the agent to put right.
fn until(params: &Map<String, Value>) -> Result<WaitUntil, String> {
    match params.get("arguments").and_then(|arguments| arguments.get("until")) {
        None | Some(Value::Null) => Ok(WaitUntil::Done),
        Some(Value::String(until)) if until == "done" => Ok(WaitUntil::Done),
        Some(Value::String(until)) if until == "checks" => Ok(WaitUntil::Checks),
        Some(other) => Err(format!("`until` is `done` or `checks`, not {other}.")),
    }
}

/// The error MCP has for a task that isn't the agent's or doesn't exist, or for the hub not saying how a wait stands,
/// which the CLI asks again after.
fn not_said(e: &ApiError) -> RpcError {
    if e.code == ErrorCode::NotFound {
        RpcError::new(Code::InvalidParams, format!("Failed to retrieve task: {}", e.message))
    } else {
        RpcError::new(Code::Internal, format!("Roundhouse couldn't say how the wait stands: {}", e.message))
    }
}

/// The wait as an MCP task, as `tasks/get` reads it.
fn task(wait: &Wait, pull_request: Option<&PullRequest>) -> Value {
    let updated_at_ms = wait.ended.map_or(wait.started_at_ms, |ended| ended.at_ms);
    let mut task = json!({
        "resultType": "complete",
        "taskId": wait.id.as_str(),
        "createdAt": iso(wait.started_at_ms),
        "lastUpdatedAt": iso(updated_at_ms),
        "ttlMs": null,
        "pollIntervalMs": POLL_INTERVAL_MS,
    });
    let (status, message) = match wait.ended {
        None => {
            let standing = pull_request::standing(Standing::of(pull_request));
            ("working", format!("Waiting on pull request #{}. {standing}", wait.number))
        }
        // The CLI says a task called off failed, and takes a turn to tell the agent what this says, but for one the
        // agent stopped itself. So it says who called it off, which says whether to start another.
        Some(ended) if ended.how.called_off() => ("cancelled", called_off(ended.how).to_string()),
        Some(ended) => {
            task["result"] = Called::text(ended_text(wait, ended.how, pull_request)).to_json();
            ("completed", format!("The wait ended: {}", how(ended.how)))
        }
    };
    task["status"] = status.into();
    task["statusMessage"] = message.into();
    task
}

/// What the agent hears as a wait ends: why, then where the pull request stands, as `get_pull_request` says it. That's
/// as last read, which for a task asked after once more has been read can be later than the read that ended it.
fn ended_text(wait: &Wait, how_it_ended: WaitEnd, pull_request: Option<&PullRequest>) -> String {
    format!(
        "The wait on pull request #{} has ended: {}\n\nAs Roundhouse last read it:\n{}",
        wait.number,
        how(how_it_ended),
        pull_request::describe(pull_request)
    )
}

/// What the agent hears of a wait that was called off, as its CLI tells it in a turn of its own.
fn called_off(how: WaitEnd) -> &'static str {
    match how {
        WaitEnd::CalledOffByPerson => "The person stopped this wait. Don't start another unless they ask.",
        WaitEnd::Replaced => "This wait was replaced by the one started after it, which goes on.",
        // By the agent, or by closing its chat or archiving its workspace, and only those are called off.
        WaitEnd::Cancelled
        | WaitEnd::Merged
        | WaitEnd::Closed
        | WaitEnd::Conflicts
        | WaitEnd::ChecksFailed
        | WaitEnd::ChangesRequested
        | WaitEnd::LeftQueue
        | WaitEnd::Draft
        | WaitEnd::ChecksPassed
        | WaitEnd::NoPullRequest
        | WaitEnd::Unknown => "The wait was called off.",
    }
}

fn how(ended: WaitEnd) -> &'static str {
    match ended {
        WaitEnd::Merged => "it's merged.",
        WaitEnd::Closed => "it was closed without being merged.",
        WaitEnd::Conflicts => "it conflicts with its base branch.",
        WaitEnd::ChecksFailed => "a check failed.",
        WaitEnd::ChangesRequested => "a reviewer asked for changes.",
        WaitEnd::LeftQueue => "it left the merge queue without merging.",
        WaitEnd::Draft => "it's a draft, which can't merge until it's marked ready.",
        WaitEnd::ChecksPassed => "its checks have run, and none failed.",
        WaitEnd::NoPullRequest => "the branch has no pull request any more, or has another one.",
        WaitEnd::Cancelled => "it was called off.",
        WaitEnd::CalledOffByPerson => "the person stopped it.",
        WaitEnd::Replaced => "the wait started after it replaced it.",
        WaitEnd::Unknown => "for a reason only a newer Roundhouse can name.",
    }
}

/// A time as MCP's tasks give one: ISO 8601, in UTC.
fn iso(at_ms: i64) -> String {
    DateTime::from_timestamp_millis(at_ms).unwrap_or_default().to_rfc3339_opts(SecondsFormat::Millis, true)
}
