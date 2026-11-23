//! What GitHub's GraphQL answers a read of repositories with, by owner and name or by ID, made from those a test sets,
//! for a hub that reads them when it likes, such as as it starts, which a script can't follow.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;
use serde_json::{Value, json};

use super::Answer;
use super::pull_requests::{aliased, answered};

/// The repositories the stand-in answers any read of repositories with, each by GitHub's ID for it, as a test sets
/// them. One a test hasn't set isn't there, and nor is one it has hidden, as GitHub answers for one a token can't see.
#[derive(Clone, Default)]
pub struct Repositories(Arc<Mutex<Set>>);

#[derive(Default)]
struct Set {
    /// Each repository's owner and name, by its ID.
    named: BTreeMap<String, String>,
    hidden: Vec<String>,
    /// The token each repository shown to only one token is shown to, by its ID, as a private one is to its owner.
    only_to: BTreeMap<String, String>,
    /// Whether every read fails, as GitHub does when it can't answer.
    failing: bool,
    /// How many queries asking after repositories it has been sent.
    queries: usize,
}

impl Repositories {
    /// The repository `id` is called `repository`, such as `example/project`, from now on, as after it's renamed or
    /// moved to another owner, when it was there already.
    pub fn set(&self, id: &str, repository: &str) {
        self.0.lock().named.insert(id.to_string(), repository.to_string());
    }

    /// GitHub shows no token the repository `id` from now on, as for a private one its account can't see, or shows it
    /// again.
    pub fn hide(&self, id: &str, hidden: bool) {
        let mut set = self.0.lock();
        set.hidden.retain(|kept| kept != id);
        if hidden {
            set.hidden.push(id.to_string());
        }
    }

    /// GitHub shows the repository `id` to `token` alone from now on, as a private one is shown to its owner's, and
    /// lists it as private.
    pub fn show_only_to(&self, id: &str, token: &str) {
        self.0.lock().only_to.insert(id.to_string(), token.to_string());
    }

    /// Every read fails from now on, with the 503 GitHub answers with when it can't, or answers again.
    pub fn fail(&self, failing: bool) {
        self.0.lock().failing = failing;
    }

    /// How many queries asking after repositories it has been sent, those it failed included.
    pub fn queries(&self) -> usize {
        self.0.lock().queries
    }

    /// What GitHub would answer the query in `body` with, from the repositories as they are, each ask by its alias.
    /// An owner and a name find a repository whatever their case, as GitHub's do.
    pub(super) fn answer(&self, token: &str, body: &str) -> Answer {
        let asked: Value = serde_json::from_str(body).unwrap_or_default();
        let query = asked["query"].as_str().unwrap_or_default();
        let mut set = self.0.lock();
        set.queries += 1;
        if set.failing {
            return Answer::status(503, Vec::new());
        }
        if query.contains("viewer { repositories(") {
            return answered(set.seen_by(token, query));
        }
        let (mut data, mut errors) = (json!({}), Vec::new());
        let by_name = aliased(query, ": repository(").into_iter().map(|(alias, asked)| {
            let quoted: Vec<&str> = asked.split('"').skip(1).step_by(2).collect();
            let named = format!("{}/{}", quoted.first().unwrap_or(&""), quoted.get(1).unwrap_or(&""));
            let id = set.named.iter().find(|(_, name)| name.eq_ignore_ascii_case(&named)).map(|(id, _)| id.clone());
            (alias, id)
        });
        let by_id = aliased(query, ": node(id:").into_iter().map(|(alias, asked)| {
            let id = asked.split('"').nth(1).unwrap_or_default();
            (alias, set.named.contains_key(id).then(|| id.to_string()))
        });
        for (alias, id) in by_name.chain(by_id).collect::<Vec<_>>() {
            if let Some(id) = id.filter(|id| set.shows(id, token)) {
                data[alias] = repository(&id, &set.named[&id]);
            } else {
                let message = "Could not resolve to a Repository.";
                errors.push(json!({"type": "NOT_FOUND", "path": [alias], "message": message}));
                data[alias] = Value::Null;
            }
        }
        let mut answer = answered(data);
        if let Answer::With { body, .. } = &mut answer
            && !errors.is_empty()
        {
            let mut with: Value = serde_json::from_str(body).unwrap_or_default();
            with["errors"] = json!(errors);
            *body = with.to_string();
        }
        answer
    }
}

impl Set {
    /// Whether GitHub shows `token` the repository `id`.
    fn shows(&self, id: &str, token: &str) -> bool {
        !self.hidden.iter().any(|hidden| hidden == id) && self.only_to.get(id).is_none_or(|only| only == token)
    }

    /// The page of the repositories `token` is shown that `query` asks for, by name, as many as its `first:` and from
    /// its `after:`, which is where the last page ended.
    fn seen_by(&self, token: &str, query: &str) -> Value {
        let number = |after: &str| after.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok();
        let first: usize = query.split("first:").nth(1).and_then(number).unwrap_or(100);
        let after: usize = query.split("after:\"").nth(1).and_then(number).unwrap_or(0);
        let mut seen: Vec<(&String, &String)> = self.named.iter().filter(|(id, _)| self.shows(id, token)).collect();
        seen.sort_by_key(|(_, name)| name.to_lowercase());
        let page: Vec<Value> = seen
            .iter()
            .skip(after)
            .take(first)
            .map(|(id, named)| {
                let mut node = repository(id, named);
                node["isPrivate"] = json!(self.only_to.contains_key(*id));
                node
            })
            .collect();
        let has_next = seen.len() > after + first;
        let page_info = json!({"hasNextPage": has_next, "endCursor": (after + first).to_string()});
        json!({"viewer": {"repositories": {"nodes": page, "pageInfo": page_info}}})
    }
}

/// The repository `id`, called `named`, as GitHub's GraphQL answers for it.
fn repository(id: &str, named: &str) -> Value {
    json!({"id": id, "nameWithOwner": named, "url": format!("https://github.com/{named}"),
        "sshUrl": format!("git@github.com:{named}.git")})
}
