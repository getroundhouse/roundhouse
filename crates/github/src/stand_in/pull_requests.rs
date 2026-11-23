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
    /// The branch of example/project each is from, `branch` unless a test says another.
    from: BTreeMap<u64, String>,
    /// The newest commit of each, `head-` and its number unless a test says another.
    heads: BTreeMap<u64, String>,
    held: Option<Gate>,
    /// Whether every read fails, as GitHub does when it can't answer.
    failing: bool,
    /// Whether example/project is hidden from the token, as a private repository its account can't see is, so GitHub
    /// answers for none.
    hidden: bool,
    /// What each answer says is left of GitHub's limit, if a test says, and otherwise most of it.
    remaining: Option<u32>,
    /// The branches that are gone from example/project, as GitHub can delete one as its pull request merges, so their
    /// pull requests are found only by the branch's name.
    deleted: Vec<String>,
}

/// What a pull request the stand-in answers for is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestState {
    /// Open, with its one check, `checks`, which `main` requires, still running.
    Open,
    /// Open, with that check passed.
    Passed,
    /// Open, with that check failed.
    Failed,
    /// A draft, with its check still running.
    Draft,
    /// Its check passed, and in the merge queue, waiting on the checks the queue runs.
    Queued,
    /// Open, with its check passed, and conflicts with `main`.
    Conflicting,
    /// Merged.
    Merged,
}

impl PullRequestState {
    /// `number`, whose newest commit is `head`, as GitHub's GraphQL answers for it in this state.
    fn json(self, number: u64, head: &str) -> Value {
        let mut pr = pull_request(number, !matches!(self, Self::Open | Self::Failed | Self::Draft), head);
        match self {
            Self::Open | Self::Passed => {}
            Self::Failed => {
                let check = &mut pr["statusCheckRollup"]["contexts"]["nodes"][0];
                (check["status"], check["conclusion"]) = (json!("COMPLETED"), json!("FAILURE"));
                pr["commits"]["nodes"][0]["commit"]["statusCheckRollup"]["state"] = json!("FAILURE");
                pr["mergeStateStatus"] = json!("BLOCKED");
            }
            Self::Draft => (pr["isDraft"], pr["mergeStateStatus"]) = (json!(true), json!("DRAFT")),
            Self::Queued => pr["mergeQueueEntry"] = json!({"state": "AWAITING_CHECKS"}),
            Self::Conflicting => (pr["mergeStateStatus"], pr["mergeable"]) = (json!("DIRTY"), json!("CONFLICTING")),
            Self::Merged => (pr["state"], pr["mergeStateStatus"]) = (json!("MERGED"), json!("UNKNOWN")),
        }
        pr
    }
}

impl PullRequests {
    /// `number` is `state` from now on.
    pub fn set(&self, number: u64, state: PullRequestState) {
        self.0.lock().states.insert(number, state);
    }

    /// `number` is `state`, from `branch` of example/project, from now on.
    pub fn set_from(&self, number: u64, state: PullRequestState, branch: &str) {
        let mut set = self.0.lock();
        set.states.insert(number, state);
        set.from.insert(number, branch.to_string());
    }

    /// `number` is `state`, from `branch` of example/project, with `head` its newest commit, as one pushed from a
    /// worktree there has, from now on.
    pub fn set_at(&self, number: u64, state: PullRequestState, branch: &str, head: &str) {
        let mut set = self.0.lock();
        set.states.insert(number, state);
        set.from.insert(number, branch.to_string());
        set.heads.insert(number, head.to_string());
    }

    /// `branch` is gone from example/project from now on, as when GitHub deleted it as its pull request merged.
    pub fn delete_branch(&self, branch: &str) {
        self.0.lock().deleted.push(branch.to_string());
    }

    /// Every read fails from now on, with the 503 GitHub answers with when it can't, or answers again.
    pub fn fail(&self, failing: bool) {
        self.0.lock().failing = failing;
    }

    /// GitHub shows the token no example/project from now on, as for a private repository its account can't see, or
    /// shows it again.
    pub fn hide(&self, hidden: bool) {
        self.0.lock().hidden = hidden;
    }

    /// Each answer says `remaining` of the limit's 5000 is left, from now on.
    pub fn leave(&self, remaining: u32) {
        self.0.lock().remaining = Some(remaining);
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
        if set.failing {
            return Answer::status(503, Vec::new());
        }
        let (mut data, mut errors) = (json!({}), Vec::new());
        for (alias, asks) in aliased(query, ": repository(") {
            if set.hidden {
                errors.push(unseen(alias));
                data[alias] = Value::Null;
                continue;
            }
            let mut parts = Vec::new();
            for (part, number) in aliased(asks, ": pullRequest(number:") {
                let number: u64 = number.split(')').next().and_then(|n| n.parse().ok()).unwrap_or_default();
                let pr = if let Some(state) = set.states.get(&number) {
                    state.json(number, &set.head(number))
                } else {
                    errors.push(
                        json!({"type":"NOT_FOUND","path":[alias,part],"message":"Could not resolve to a PullRequest."}),
                    );
                    Value::Null
                };
                parts.push((part.to_string(), pr));
            }
            for (part, asked) in aliased(asks, ": ref(") {
                let branch = asked.strip_prefix("qualifiedName:").and_then(|asked| asked.split(')').next());
                let branch: String = branch.and_then(|branch| serde_json::from_str(branch).ok()).unwrap_or_default();
                let branch = branch.strip_prefix("refs/heads/").unwrap_or_default();
                // A branch's pull requests are asked for of the branch itself, and `main`'s rules of `main`.
                let answer =
                    match (asked.contains("associatedPullRequests"), set.deleted.iter().any(|gone| gone == branch)) {
                        (false, _) => rules(),
                        (true, true) => Value::Null,
                        (true, false) => json!({ "associatedPullRequests": set.heads(branch) }),
                    };
                parts.push((part.to_string(), answer));
            }
            for (part, branch) in aliased(asks, ": pullRequests(headRefName:") {
                let branch: String =
                    serde_json::from_str(branch.split(',').next().unwrap_or_default()).unwrap_or_default();
                parts.push((part.to_string(), set.heads(&branch)));
            }
            data[alias] = repository(&parts);
        }
        let mut answer = answered(data);
        if let Answer::With { body, .. } = &mut answer {
            let mut with: Value = serde_json::from_str(body).unwrap_or_default();
            if !errors.is_empty() {
                with["errors"] = json!(errors);
            }
            if let Some(remaining) = set.remaining {
                with["data"]["rateLimit"]["remaining"] = json!(remaining);
            }
            *body = with.to_string();
        }
        match &set.held {
            Some(gate) => Answer::After(gate.clone(), Box::new(answer)),
            None => answer,
        }
    }
}

impl Set {
    fn head(&self, number: u64) -> String {
        self.heads.get(&number).cloned().unwrap_or_else(|| format!("head-{number}"))
    }

    /// The pull requests from `branch`, the newest first, as a branch's are asked for, by the branch or by its name.
    fn heads(&self, branch: &str) -> Value {
        let nodes: Vec<Value> = self
            .states
            .iter()
            .rev()
            .filter(|(number, _)| self.from.get(number).map_or("branch", String::as_str) == branch)
            .map(|(number, state)| {
                let state = if *state == PullRequestState::Merged { "MERGED" } else { "OPEN" };
                json!({"number": number, "url": format!("https://github.com/example/project/pull/{number}"),
                    "state": state, "headRefOid": self.head(*number),
                    "headRepository": {"nameWithOwner": "example/project"},
                    "baseRepository": {"nameWithOwner": "example/project"}})
            })
            .collect();
        json!({ "nodes": nodes })
    }
}

/// The error GitHub gives about the repository asked of as `alias` when it doesn't show the token one.
fn unseen(alias: &str) -> Value {
    let message = "Could not resolve to a Repository with the name 'example/project'.";
    json!({"type": "NOT_FOUND", "path": [alias], "message": message})
}

/// Each alias in `query` before `marker`, with what follows it up to the next, as `r0: repository(` names the
/// repository a query asks of first, and what it asks of it.
pub(super) fn aliased<'a>(query: &'a str, marker: &str) -> Vec<(&'a str, &'a str)> {
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
            let pr = pull_request(*number, *passed, &format!("head-{number}"));
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

pub(super) fn answered(mut data: Value) -> Answer {
    data["rateLimit"] = json!({"cost":1,"limit":5000,"remaining":4900,"resetAt":"2099-01-01T00:00:00Z"});
    Answer::json(&json!({ "data": data }))
}

fn pull_request(number: u64, passed: bool, head: &str) -> Value {
    let check = json!({"__typename":"CheckRun","name":"checks","url":null,"isRequired":true,
        "status":if passed {"COMPLETED"} else {"IN_PROGRESS"},"conclusion":if passed {json!("SUCCESS")} else {Value::Null},
        "startedAt":"2026-01-01T00:00:00Z","checkSuite":{"createdAt":"2026-01-01T00:00:00Z","commit":{"oid":head},
        "app":{"databaseId":1},"workflowRun":{"workflow":{"id":"workflow"}}}});
    json!({"number":number,"url":format!("https://github.com/example/project/pull/{number}"),"title":format!("Change {number}"),
        "state":"OPEN","isDraft":false,"headRefOid":head,"baseRefOid":"base","baseRefName":"main","headRefName":"branch",
        "mergeStateStatus":"CLEAN","mergeable":"MERGEABLE","reviewDecision":null,"potentialMergeCommit":null,
        "mergeQueueEntry":null,"reviews":{"nodes":[]},"timelineItems":{"nodes":[]},"stackEntry":null,"stack":null,
        "createdAt":"2026-01-01T00:00:00Z","author":{"login":"ada"},"comments":{"totalCount":0},"bodyText":"",
        "labels":{"nodes":[]},"additions":1,"deletions":0,
        "commits":{"nodes":[{"commit":{"statusCheckRollup":{"state":if passed {"SUCCESS"} else {"PENDING"}}}}]},
        "statusCheckRollup":{"commit":{"oid":head},"contexts":{"nodes":[check],"pageInfo":{"hasNextPage":false,"endCursor":null}}}})
}
