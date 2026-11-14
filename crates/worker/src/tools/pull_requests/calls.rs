use super::{
    Asks, Called, CheckState, Map, PullRequestSelection, PullRequestWatch, PullRequestsSnapshot, ReadPullRequests,
    SessionId, StartPullRequestWait, Value, Wait, WaitEnd, WaitUntil, json,
};

pub(in crate::tools) const READ: &str = "get_pull_requests";
pub(in crate::tools) const WAIT: &str = protocol::worker::WAITS_TOOL;

pub(in crate::tools) fn definition(wait: bool) -> Value {
    let mut properties = json!({ "pull_requests": {
        "type": "array", "items": { "type": "integer", "minimum": 1, "maximum": 2_147_483_647 },
        "minItems": 1, "uniqueItems": true,
        "description": "PR numbers in this workspace's repository. Open PRs below them in their GitHub stacks are included. Give a stack's top PR to cover the stack; order does not matter."
    }});
    let mut required = vec!["pull_requests"];
    if wait {
        properties["until"] = json!({ "type": "string", "enum": ["required_checks", "all_checks", "merged"],
            "description": "required_checks: every watched PR's effective required checks are satisfied; optional checks may still run. all_checks: all checks currently reported for the latest revisions finish without failure. merged: every watched PR has merged; entering the queue is not completion. Every mode ends early for a blocker or new request for changes." });
        properties["timeout_seconds"] = json!({ "type": "integer", "minimum": 1,
            "description": "Optional maximum lifetime of the background wait, including time while disconnected. Omit to wait without a deadline. Expiry returns timed_out and the latest known state." });
        required.push("until");
    }
    json!({
        "name": if wait { WAIT } else { READ },
        "title": if wait { "Wait on pull requests" } else { "Get pull requests" },
        "description": if wait {
            "Waits on explicit pull requests and their open lower stack dependencies. Returns a background task and says which PRs it watches. All must reach the condition; one conflict, required rebase, relevant failed check, new change request, closure without merging, or queue rejection ends the wait early. Follows new pushed revisions without switching branches. A changed dependency set ends as stack_changed. You may end your turn. Use this instead of polling GitHub or sleeping. Each call is independent: it does not replace another wait. It never rebases, pushes, queues or merges anything."
        } else {
            "Reads explicit pull requests and their open lower GitHub stack dependencies, using this workspace's repository and credentials. Returns current revisions, checks and effective requirements, mergeability, queue and review state, and the time read. Uses the same scope as wait_for_pull_requests. Does not switch branches. A PR in the queue has not yet merged. Use the wait tool instead of repeatedly reading."
        },
        "inputSchema": { "type": "object", "properties": properties, "required": required, "additionalProperties": false },
        "annotations": { "readOnlyHint": true, "openWorldHint": true }
    })
}

pub(in crate::tools) async fn read(asks: &Asks, session: &SessionId, params: &Map<String, Value>) -> Value {
    let selection = read_arguments(params);
    let selection = match selection {
        Ok(selection) => selection,
        Err(error) => return Called::failed(error).to_json(),
    };
    match asks.ask(session.clone(), ReadPullRequests { pull_requests: selection }).await {
        Ok(snapshot) => {
            let mut result = Called::text(describe(&snapshot)).to_json();
            result["structuredContent"] = json!(snapshot);
            result
        }
        Err(error) => {
            Called::failed(format!("Roundhouse couldn't read the pull requests: {}", error.message)).to_json()
        }
    }
}

pub(in crate::tools) async fn start(asks: &Asks, session: &SessionId, params: &Map<String, Value>) -> Value {
    let asked = match arguments(params) {
        Ok(asked) => asked,
        Err(error) => return Called::failed(error).to_json(),
    };
    match asks.ask(session.clone(), asked).await {
        Ok(answer) => {
            let mut task = super::super::wait::task(&answer.wait, answer.pull_request.as_ref());
            if answer.wait.ended.is_none() {
                task["resultType"] = "task".into();
            } else {
                return result(&answer.wait);
            }
            task
        }
        Err(error) => Called::failed(format!("Roundhouse couldn't start the wait: {}", error.message)).to_json(),
    }
}

pub(super) fn read_arguments(params: &Map<String, Value>) -> Result<PullRequestSelection, String> {
    let args = params.get("arguments").and_then(Value::as_object).ok_or("give pull_requests")?;
    if args.keys().any(|key| key != "pull_requests") {
        return Err("unknown read argument".into());
    }
    selection(params)
}

fn selection(params: &Map<String, Value>) -> Result<PullRequestSelection, String> {
    let args = params.get("arguments").ok_or("give pull_requests")?;
    serde_json::from_value(args["pull_requests"].clone()).map_err(|error| error.to_string())
}

pub(super) fn arguments(params: &Map<String, Value>) -> Result<StartPullRequestWait, String> {
    let args = params.get("arguments").and_then(Value::as_object).ok_or("give pull_requests and until")?;
    if args.keys().any(|key| !["pull_requests", "until", "timeout_seconds"].contains(&key.as_str())) {
        return Err("unknown wait argument".into());
    }
    let pull_requests = selection(params)?;
    let until =
        serde_json::from_value(args.get("until").cloned().unwrap_or(Value::Null)).map_err(|error| error.to_string())?;
    let timeout_seconds = args
        .get("timeout_seconds")
        .map(|value| {
            value
                .as_u64()
                .filter(|seconds| *seconds > 0)
                .ok_or_else(|| "timeout_seconds must be a positive integer".to_owned())
        })
        .transpose()?;
    let call =
        params.get("_meta").and_then(|meta| meta.get("claudecode/toolUseId")).and_then(Value::as_str).map(Into::into);
    Ok(StartPullRequestWait { pull_requests, until, timeout_seconds, call })
}

pub(in crate::tools) fn progress(watch: &PullRequestWatch) -> String {
    let numbers = watch
        .snapshot
        .pull_requests
        .iter()
        .map(|pr| format!("#{}", pr.pull_request.number))
        .collect::<Vec<_>>()
        .join(", ");
    match &watch.read_error {
        Some(error) => {
            format!(
                "Waiting on {numbers}. Last successful read: {}. {error}",
                super::super::wait::iso(watch.snapshot.observed_at_ms)
            )
        }
        None => format!("Waiting on {numbers}. {}", counts(&watch.snapshot)),
    }
}

pub(in crate::tools) fn ending(wait: &Wait, reason: WaitEnd) -> &'static str {
    match (reason, wait.until) {
        (WaitEnd::ChecksPassed, WaitUntil::RequiredChecks) => "every watched PR's required checks are satisfied",
        (WaitEnd::ChecksPassed, WaitUntil::AllChecks) => "all currently reported checks finished without failure",
        (WaitEnd::Merged, _) => "every watched PR has merged",
        _ => super::super::wait::how(reason),
    }
}

pub(in crate::tools) fn result(wait: &Wait) -> Value {
    let Some(result) = wait.result() else {
        return Called::failed("This wait has no terminal result.".into()).to_json();
    };
    let affected = result.watch.affected.iter().map(|n| format!("#{}", n.get())).collect::<Vec<_>>().join(", ");
    let reason = ending(wait, result.reason);
    let prefix =
        if affected.is_empty() { "The wait ended".to_owned() } else { format!("The wait ended for {affected}") };
    let mut text = format!("{prefix}: {reason}\n\n{}", describe(&result.watch.snapshot));
    if let Some(error) = &result.watch.read_error {
        text.push_str("\nLast read error: ");
        text.push_str(error);
    }
    if result.reason == protocol::model::WaitEnd::CalledOffByPerson {
        text.push_str("\nDon't start another unless the person asks.");
    }
    let mut called = Called::text(text).to_json();
    called["structuredContent"] = json!(result);
    called
}

fn describe(snapshot: &PullRequestsSnapshot) -> String {
    let mut lines = vec![format!(
        "{} — read at {}. {}",
        snapshot.repository.name_with_owner,
        super::super::wait::iso(snapshot.observed_at_ms),
        counts(snapshot)
    )];
    for row in &snapshot.pull_requests {
        let pr = &row.pull_request;
        lines.push(format!("#{}: {} — {}\nHead: {}. Destination: {}. State: {:?}. Mergeability: {:?}. Linear: {:?}. Queue: {:?}. Review: {:?}.",
            pr.number,pr.title,pr.url,row.head,row.destination,pr.state,row.mergeability,row.linear,pr.queue,pr.review));
        for check in &row.checks {
            lines.push(format!(
                "  {}: {:?}{}{}",
                check.name,
                check.state,
                if check.required { " (required)" } else { " (optional)" },
                check.url.as_ref().map_or_else(String::new, |url| format!(" — {url}"))
            ));
        }
    }
    lines.join("\n")
}

fn counts(snapshot: &PullRequestsSnapshot) -> String {
    let (mut passed, mut pending, mut failed) = (0, 0, 0);
    for check in snapshot.pull_requests.iter().flat_map(|pr| &pr.checks) {
        match check.state {
            CheckState::Passed => passed += 1,
            CheckState::Failed => failed += 1,
            CheckState::Pending | CheckState::Unknown => pending += 1,
        }
    }
    format!("Checks: {passed} passed, {pending} running or unknown, {failed} failed.")
}
