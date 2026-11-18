use super::super::mcp::{Called, Code, RpcError};
use super::super::pull_request;
use crate::asks::Asks;
use jiff::Timestamp;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{PullRequest, SessionId, Standing, Wait, WaitEnd};
use protocol::worker::{CancelWait, ReadWait, WaitAnswer};
use serde_json::{Value, json};
use std::time::Duration;

/// `tasks/get`: how the wait stands, with how it ended once it has.
pub(in super::super) async fn get(
    asks: &Asks,
    wait_poll: Duration,
    session_id: &SessionId,
    task_id: &str,
) -> Result<Value, RpcError> {
    let asked = asks.ask(session_id.clone(), ReadWait { wait: task_id.into() }).await;
    let WaitAnswer { wait, pull_request } = asked.map_err(|e| not_said(&e))?;
    Ok(task(&wait, pull_request.as_ref(), wait_poll))
}

/// `tasks/cancel`: ends the wait, unless it has ended already.
pub(in super::super) async fn cancel(asks: &Asks, session_id: &SessionId, task_id: &str) -> Result<Value, RpcError> {
    asks.ask(session_id.clone(), CancelWait { wait: task_id.into() }).await.map_err(|e| not_said(&e))?;
    Ok(json!({ "resultType": "complete" }))
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

/// The wait as an MCP task, as `tasks/get` reads it, which the CLI is to ask after every `wait_poll`.
pub(in crate::tools) fn task(wait: &Wait, pull_request: Option<&PullRequest>, wait_poll: Duration) -> Value {
    let updated_at_ms = wait.ended.map_or_else(
        || wait.watch.as_ref().map_or(wait.started_at_ms, |watch| watch.last_attempt_at_ms),
        |ended| ended.at_ms,
    );
    let mut task = json!({
        "resultType": "complete",
        "taskId": wait.id.as_str(),
        "createdAt": iso(wait.started_at_ms),
        "lastUpdatedAt": iso(updated_at_ms),
        "ttlMs": null,
        "pollIntervalMs": u64::try_from(wait_poll.as_millis()).unwrap_or(u64::MAX),
    });
    let (status, message) = match wait.ended {
        None => {
            if let Some(watch) = &wait.watch {
                task["status"] = "working".into();
                task["statusMessage"] = crate::tools::pull_requests::progress(watch).into();
                return task;
            }
            let standing = pull_request::standing(Standing::of(pull_request));
            ("working", format!("Waiting on pull request #{}. {standing}", wait.number))
        }
        // The CLI says a task called off failed, and takes a turn to tell the agent what this says, but for one the
        // agent stopped itself. So it says who called it off, which says whether to start another.
        Some(ended) if ended.how.called_off() => {
            if wait.watch.is_some() {
                task["result"] = crate::tools::pull_requests::result(wait);
            }
            ("cancelled", called_off(ended.how).to_string())
        }
        Some(ended) => {
            task["result"] = if wait.watch.is_some() {
                crate::tools::pull_requests::result(wait)
            } else {
                Called::text(ended_text(wait, ended.how, pull_request)).to_json()
            };
            let reason = if wait.watch.is_some() {
                crate::tools::pull_requests::ending(wait, ended.how)
            } else {
                how(ended.how)
            };
            ("completed", format!("The wait ended: {reason}"))
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
        | WaitEnd::StackChanged
        | WaitEnd::NeedsRebase
        | WaitEnd::TimedOut
        | WaitEnd::Unavailable
        | WaitEnd::Unknown => "The wait was called off.",
    }
}

pub(in crate::tools) fn how(ended: WaitEnd) -> &'static str {
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
        WaitEnd::StackChanged => "its stack dependencies or destination changed.",
        WaitEnd::NeedsRebase => "its stack needs rebasing.",
        WaitEnd::TimedOut => "the requested deadline was reached.",
        WaitEnd::Unavailable => "GitHub is no longer accessible for this wait.",
        WaitEnd::Unknown => "for a reason only a newer Roundhouse can name.",
    }
}

/// A time as MCP's tasks give one: ISO 8601, in UTC.
pub(in crate::tools) fn iso(at_ms: i64) -> String {
    format!("{:.3}", Timestamp::from_millisecond(at_ms).unwrap_or_default())
}
