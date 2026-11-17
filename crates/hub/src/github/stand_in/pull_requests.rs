//! What GitHub's GraphQL answers a read of pull requests with, for a test whose read needs only answering, not
//! taking apart, which the hub's own tests of reading do: scripted answers in turn, or answers made from pull requests
//! a test sets, for a hub that reads them when it likes, such as in the background, which a script can't follow.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};

use super::{Answer, Gate};

/// The pull requests of example/project the stand-in answers any read of, as a test sets them. One a test hasn't set
/// isn't there, as GitHub answers for a number it has no pull request of.
#[derive(Clone, Default)]
pub struct PullRequests(Arc<Mutex<Set>>);

#[derive(Default)]
struct Set {
    states: BTreeMap<u64, PullRequestState>,
    held: Option<Gate>,
}

/// What a pull request the stand-in answers for is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestState {
    /// Open, with its one check, `checks`, which `main` requires, still running.
    Open,
    /// Open, with that check passed.
    Passed,
    /// Merged.
    Merged,
}

impl PullRequests {
    /// `number` is `state` from now on.
    pub fn set(&self, number: u64, state: PullRequestState) {
        self.0.lock().states.insert(number, state);
    }

    /// Holds every read back, as they are when asked, until [`PullRequests::release`].
    pub fn hold(&self) {
        self.0.lock().held = Some(Gate::closed());
    }

    pub fn release(&self) {
        if let Some(gate) = self.0.lock().held.take() {
            gate.open();
        }
    }

    /// What GitHub would answer the query in `body` with, from the pull requests as they are, held back while reads
    /// are. Each repository asked of is example/project, and each part of it is answered by its alias, as the hub's
    /// queries name them.
    pub(super) fn answer(&self, body: &str) -> Answer {
        let asked: Value = serde_json::from_str(body).unwrap_or_default();
        let query = asked["query"].as_str().unwrap_or_default();
        let set = self.0.lock();
        let (mut data, mut errors) = (json!({}), Vec::new());
        for (alias, asks) in aliased(query, ": repository(") {
            let mut parts = Vec::new();
            for (part, number) in aliased(asks, ": pullRequest(number:") {
                let number: u64 = number.split(')').next().and_then(|n| n.parse().ok()).unwrap_or_default();
                let pr = match set.states.get(&number) {
                    Some(PullRequestState::Open) => pull_request(number, false),
                    Some(PullRequestState::Passed) => pull_request(number, true),
                    Some(PullRequestState::Merged) => merged(number),
                    None => {
                        errors.push(json!({"type":"NOT_FOUND","path":[alias,part],"message":"Could not resolve to a PullRequest."}));
                        Value::Null
                    }
                };
                parts.push((part.to_string(), pr));
            }
            for (part, _) in aliased(asks, ": ref(") {
                parts.push((part.to_string(), rules()));
            }
            data[alias] = repository(&parts);
        }
        let mut answer = answered(data);
        if let (Answer::With { body, .. }, false) = (&mut answer, errors.is_empty()) {
            let mut with: Value = serde_json::from_str(body).unwrap_or_default();
            with["errors"] = json!(errors);
            *body = with.to_string();
        }
        match &set.held {
            Some(gate) => Answer::After(gate.clone(), Box::new(answer)),
            None => answer,
        }
    }
}

/// Each alias in `query` before `marker`, with what follows it up to the next, as `r0: repository(` names the
/// repository a query asks of first, and what it asks of it.
fn aliased<'a>(query: &'a str, marker: &str) -> Vec<(&'a str, &'a str)> {
    let starts: Vec<usize> = query.match_indices(marker).map(|(at, _)| at).collect();
    starts
        .iter()
        .enumerate()
        .map(|(index, &at)| {
            let alias = query[..at].rsplit(char::is_whitespace).next().unwrap_or_default();
            let end = starts.get(index + 1).copied().unwrap_or(query.len());
            (alias, &query[at + marker.len()..end])
        })
        .collect()
}

fn rules() -> Value {
    json!({"branchProtectionRule":null,"refUpdateRule":null,"rules":{"nodes":[{"type":"REQUIRED_STATUS_CHECKS",
        "repositoryRuleset":{"enforcement":"ACTIVE"},"parameters":{"requiredStatusChecks":[{"context":"checks","integrationId":1}]}}],
        "pageInfo":{"hasNextPage":false,"endCursor":null}}})
}

fn merged(number: u64) -> Value {
    let mut pr = pull_request(number, true);
    pr["state"] = json!("MERGED");
    pr["mergeStateStatus"] = json!("UNKNOWN");
    pr
}

impl Answer {
    /// The two queries a read of one open pull request, `number` of example/project, makes when it's in no stack: it
    /// with its checks, then it again, confirmed, with `main`'s rules, which require `checks` from the app numbered
    /// 1. Its one check is `checks`, which has passed or is still running, as `passed` says.
    pub fn pull_request_read(number: u64, passed: bool) -> Vec<Answer> {
        Self::pull_requests_read(&[(number, passed)])
    }

    /// The same, for a read of as many asks together, each of one pull request as `asked` has it, in turn.
    pub fn pull_requests_read(asked: &[(u64, bool)]) -> Vec<Answer> {
        let rules = rules();
        let (mut looked, mut confirmed) = (json!({}), json!({}));
        for (at, (number, passed)) in asked.iter().enumerate() {
            let pr = pull_request(*number, *passed);
            looked[format!("r{at}")] = repository(&[(format!("p{number}"), pr.clone())]);
            confirmed[format!("r{at}")] = repository(&[("b0".to_string(), rules.clone()), (format!("v{number}"), pr)]);
        }
        vec![answered(looked), answered(confirmed)]
    }
}

fn repository(parts: &[(String, Value)]) -> Value {
    let mut repository = json!({"nameWithOwner":"example/project","url":"https://github.com/example/project"});
    for (alias, part) in parts {
        repository[alias] = part.clone();
    }
    repository
}

fn answered(mut data: Value) -> Answer {
    data["rateLimit"] = json!({"cost":1,"limit":5000,"remaining":4900,"resetAt":"2099-01-01T00:00:00Z"});
    Answer::json(&json!({ "data": data }))
}

fn pull_request(number: u64, passed: bool) -> Value {
    let head = format!("head-{number}");
    let check = json!({"__typename":"CheckRun","name":"checks","url":null,"isRequired":true,
        "status":if passed {"COMPLETED"} else {"IN_PROGRESS"},"conclusion":if passed {json!("SUCCESS")} else {Value::Null},
        "startedAt":"2026-01-01T00:00:00Z","checkSuite":{"createdAt":"2026-01-01T00:00:00Z","commit":{"oid":head},
        "app":{"databaseId":1},"workflowRun":{"workflow":{"id":"workflow"}}}});
    json!({"number":number,"url":format!("https://github.com/example/project/pull/{number}"),"title":format!("Change {number}"),
        "state":"OPEN","isDraft":false,"headRefOid":head,"baseRefOid":"base","baseRefName":"main","headRefName":"branch",
        "mergeStateStatus":"CLEAN","mergeable":"MERGEABLE","reviewDecision":null,"potentialMergeCommit":null,
        "mergeQueueEntry":null,"reviews":{"nodes":[]},"timelineItems":{"nodes":[]},"stackEntry":null,"stack":null,
        "statusCheckRollup":{"commit":{"oid":head},"contexts":{"nodes":[check],"pageInfo":{"hasNextPage":false,"endCursor":null}}}})
}
