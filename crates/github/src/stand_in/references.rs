//! What GitHub's GraphQL answers a read of issues and pull requests by number with, made from those a test sets, for
//! a hub that reads them when it likes, such as as a reply links to them, which a script can't follow.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use protocol::model::{
    ChecksRollup, GithubRepositoryName, IssueNumber, IssueState, PullRequestState, Reference, ReferenceKind,
};
use serde_json::{Value, json};

use super::pull_requests::{aliased, answered};
use super::{Answer, Gate};

/// The issues and pull requests the stand-in answers any read of them with, as a test sets them. A number a test
/// hasn't set isn't there, and nor is a repository it has set none in, as GitHub answers for those a token can't see.
#[derive(Clone, Default)]
pub struct References(Arc<Mutex<Set>>);

#[derive(Default)]
struct Set {
    /// By repository, lowercased, as GitHub finds one whatever case it's asked in, and then by number.
    references: BTreeMap<(String, IssueNumber), Reference>,
    /// Whether every read fails, as GitHub does when it can't answer.
    failing: bool,
    /// How many queries asking after them it has been sent.
    queries: usize,
    /// What holds every answer back while reads are held, as [`References::hold`] says.
    held: Option<Gate>,
}

impl References {
    /// `reference` is there from now on, in place of whatever was at its repository and number, and GitHub answers
    /// for it as it says, its repository named as it has it.
    pub fn set(&self, reference: Reference) {
        let key = (reference.repository.as_str().to_lowercase(), reference.number);
        self.0.lock().references.insert(key, reference);
    }

    /// `number` of `repository` isn't there from now on.
    pub fn remove(&self, repository: &GithubRepositoryName, number: IssueNumber) {
        self.0.lock().references.remove(&(repository.as_str().to_lowercase(), number));
    }

    /// Every read fails from now on, with the 503 GitHub answers with when it can't, or answers again.
    pub fn fail(&self, failing: bool) {
        self.0.lock().failing = failing;
    }

    /// Holds every read back, as they are when asked, until [`References::release`].
    pub fn hold(&self) {
        self.0.lock().held = Some(Gate::closed());
    }

    pub fn release(&self) {
        if let Some(gate) = self.0.lock().held.take() {
            gate.open();
        }
    }

    /// How many queries asking after issues or pull requests it has been sent, those it failed included, so a test
    /// can tell what was read again from what was kept.
    pub fn queries(&self) -> usize {
        self.0.lock().queries
    }

    /// What GitHub would answer the query in `body` with, from the issues and pull requests as they are. Each
    /// repository and each number in it is answered by its alias, as the hub's queries name them.
    pub(super) fn answer(&self, body: &str) -> Answer {
        let asked: Value = serde_json::from_str(body).unwrap_or_default();
        let query = asked["query"].as_str().unwrap_or_default();
        let mut set = self.0.lock();
        set.queries += 1;
        let answer = set.answer(query);
        match &set.held {
            Some(gate) => Answer::After(gate.clone(), Box::new(answer)),
            None => answer,
        }
    }
}

impl Set {
    /// What GitHub answers `query` with, from the issues and pull requests as they are.
    fn answer(&self, query: &str) -> Answer {
        if self.failing {
            return Answer::status(503, Vec::new());
        }
        let (mut data, mut errors) = (json!({}), Vec::new());
        for (alias, asks) in aliased(query, ": repository(") {
            let named = repository(asks);
            let mut there = self.references.iter().filter(|((repository, _), _)| *repository == named);
            let Some((_, first)) = there.next() else {
                data[alias] = Value::Null;
                let message = format!("Could not resolve to a Repository with the name '{named}'.");
                errors.push(json!({"type": "NOT_FOUND", "path": [alias], "message": message}));
                continue;
            };
            data[alias] = json!({ "nameWithOwner": first.repository.as_str() });
            for (part, number) in aliased(asks, ": issueOrPullRequest(number:") {
                let number = number.split(')').next().and_then(|n| n.parse::<u64>().ok()).unwrap_or_default();
                let found = IssueNumber::try_from(number).ok().and_then(|n| self.references.get(&(named.clone(), n)));
                data[alias][part] = found.map_or(Value::Null, node);
                if found.is_none() {
                    let message = format!("Could not resolve to an issue or pull request with the number of {number}.");
                    errors.push(json!({"type": "NOT_FOUND", "path": [alias, part], "message": message}));
                }
            }
        }
        let mut answer = answered(data);
        if let Answer::With { body, .. } = &mut answer
            && !errors.is_empty()
        {
            let mut with: Value = serde_json::from_str(body).unwrap();
            with["errors"] = json!(errors);
            *body = with.to_string();
        }
        answer
    }
}

/// The repository a query's `repository(owner:…,name:…)` asks of, from what follows its opening bracket, lowercased.
fn repository(asks: &str) -> String {
    let (owner, rest) = asks.strip_prefix("owner:").and_then(|asks| asks.split_once(",name:")).unwrap_or_default();
    let name = rest.split(')').next().unwrap_or_default();
    let read = |quoted: &str| serde_json::from_str::<String>(quoted).unwrap_or_default();
    format!("{}/{}", read(owner), read(name)).to_lowercase()
}

/// `reference` as GitHub's `issueOrPullRequest` answers for it.
fn node(reference: &Reference) -> Value {
    let labels: Vec<Value> =
        reference.labels.iter().map(|label| json!({"name": label.name, "color": label.color})).collect();
    let opened = jiff::Timestamp::from_millisecond(reference.opened_at_ms).unwrap();
    let author = reference.author.as_ref().map(|login| json!({ "login": login }));
    let mut node = json!({"number": reference.number.get(), "url": reference.url, "title": reference.title,
        "createdAt": opened.to_string(), "author": author,
        "comments": {"totalCount": reference.comments}, "bodyText": reference.body, "labels": {"nodes": labels}});
    match reference.kind {
        ReferenceKind::Issue { state } => {
            let (state, reason) = match state {
                IssueState::Open => ("OPEN", None),
                IssueState::Done => ("CLOSED", Some("COMPLETED")),
                IssueState::NotPlanned => ("CLOSED", Some("NOT_PLANNED")),
                IssueState::Unknown => ("UNKNOWN", None),
            };
            (node["__typename"], node["state"], node["stateReason"]) = (json!("Issue"), json!(state), json!(reason));
        }
        ReferenceKind::PullRequest(pull_request) => {
            let (state, draft) = match pull_request.state {
                PullRequestState::Open => ("OPEN", false),
                PullRequestState::Draft => ("OPEN", true),
                PullRequestState::Merged => ("MERGED", false),
                PullRequestState::Closed => ("CLOSED", false),
                PullRequestState::Unknown => ("UNKNOWN", false),
            };
            let review = serde_json::to_value(pull_request.review).unwrap();
            let rollup = pull_request.checks.map(|checks| json!({ "state": state_of(checks) }));
            node["__typename"] = json!("PullRequest");
            node["state"] = json!(state);
            node["isDraft"] = json!(draft);
            node["additions"] = json!(pull_request.added);
            node["deletions"] = json!(pull_request.removed);
            node["reviewDecision"] = if review == "" { Value::Null } else { review };
            node["commits"] = json!({"nodes": [{"commit": {"statusCheckRollup": rollup}}]});
        }
        // What GitHub might one day answer that this stand-in can't say.
        ReferenceKind::Unknown => node["__typename"] = json!("Discussion"),
    }
    node
}

/// GitHub's `StatusState` for `checks`.
fn state_of(checks: ChecksRollup) -> &'static str {
    match checks {
        ChecksRollup::Passed => "SUCCESS",
        ChecksRollup::Failed => "FAILURE",
        ChecksRollup::Running => "PENDING",
        ChecksRollup::Unknown => "UNKNOWN",
    }
}
