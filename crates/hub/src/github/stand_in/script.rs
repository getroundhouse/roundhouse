use std::collections::{HashMap, VecDeque};

use super::Answer;
use super::apps::{as_the_app, named};

/// What the stand-in answers each call with. A call it has run out of answers for gets the last again.
#[derive(Default)]
pub struct Script {
    /// `GET /user`, by the token asked with. Any other is a 401, as GitHub answers a token it doesn't know.
    pub users: Vec<(&'static str, Answer)>,
    /// `POST /login/device/code`, in turn.
    pub codes: Vec<Answer>,
    /// `POST /login/oauth/access_token`, in turn, by the device code asked with. Any other is GitHub's
    /// `incorrect_device_code`.
    pub polls: Vec<(&'static str, Vec<Answer>)>,
    /// `POST /login/oauth/access_token` renewing a sign-in's token, in turn, by what renews it asked with. Any other is
    /// GitHub's `bad_refresh_token`, as it answers one it doesn't know, or that has ended, been revoked or been used.
    pub renewals: Vec<(&'static str, Vec<Answer>)>,
    /// `POST /app-manifests/{code}/conversions`, by the code, each once, as GitHub takes a code. Any other, or one
    /// asked again, is a 404, as GitHub answers a code it doesn't know.
    pub conversions: Vec<(&'static str, Answer)>,
    /// `GET /users/{name}`, by the name, such as `ada-roundhouse[bot]`. Any other is a 404.
    pub named: Vec<(&'static str, Answer)>,
    /// `GET /app/installations`, in turn, to a JWT signed with [`MADE_UP_APP_KEY`](super::MADE_UP_APP_KEY) that
    /// hasn't ended. Any other JWT is a 401, as GitHub answers one it doesn't take.
    pub installations: Vec<Answer>,
    /// `POST /app/installations/{id}/access_tokens`, in turn, by the installation, to a JWT as `installations` takes
    /// one. Any other installation is a 404, as GitHub answers one that's gone.
    pub minted: Vec<(u64, Vec<Answer>)>,
    /// `GET /{owner}/{name}.git/info/refs?service=git-receive-pack`, which `git push` asks first, by the repository
    /// asked of, such as `cli/cli`, in turn, whatever credential it's asked with. Any other is a 404, as GitHub answers a
    /// repository it doesn't have.
    pub pushes: Vec<(&'static str, Vec<Answer>)>,
}

/// What it answers with still, as it goes through the script.
pub(super) struct Answering {
    users: HashMap<&'static str, Answer>,
    codes: VecDeque<Answer>,
    polls: HashMap<String, VecDeque<Answer>>,
    renewals: HashMap<String, VecDeque<Answer>>,
    pub(super) conversions: HashMap<String, Answer>,
    pub(super) named: HashMap<String, Answer>,
    pub(super) installations: VecDeque<Answer>,
    pub(super) minted: HashMap<u64, VecDeque<Answer>>,
    pushes: HashMap<&'static str, VecDeque<Answer>>,
}

impl Answering {
    pub(super) fn of(script: Script) -> Self {
        let in_turn = |(asked, answers): (&str, Vec<Answer>)| (asked.to_string(), answers.into());
        Self {
            users: script.users.into_iter().collect(),
            codes: script.codes.into(),
            polls: script.polls.into_iter().map(in_turn).collect(),
            renewals: script.renewals.into_iter().map(in_turn).collect(),
            conversions: script.conversions.into_iter().map(|(code, answer)| (code.to_string(), answer)).collect(),
            named: script.named.into_iter().map(|(name, answer)| (name.to_string(), answer)).collect(),
            installations: script.installations.into(),
            minted: script.minted.into_iter().map(|(id, answers)| (id, answers.into())).collect(),
            pushes: script.pushes.into_iter().map(|(repository, answers)| (repository, answers.into())).collect(),
        }
    }

    pub(super) fn next(answers: &mut VecDeque<Answer>) -> Option<Answer> {
        if answers.len() > 1 { answers.pop_front() } else { answers.front().cloned() }
    }
}

/// What the script has for the request whose head is `head`, and whose body is `body`.
pub(super) fn chosen(answering: &mut Answering, head: &str, body: &str) -> Answer {
    if let Some(answer) = as_the_app(answering, head).or_else(|| named(answering, head)) {
        return answer;
    }
    if head.starts_with("POST /login/device/code ") {
        return Answering::next(&mut answering.codes).unwrap_or_else(|| Answer::status(404, Vec::new()));
    }
    if head.starts_with("POST /login/oauth/access_token ") {
        let asked: serde_json::Value = serde_json::from_str(body).unwrap();
        if let Some(refresh) = asked["refresh_token"].as_str() {
            let renewals = answering.renewals.get_mut(refresh);
            return renewals
                .and_then(Answering::next)
                .unwrap_or_else(|| Answer::device_flow_error("bad_refresh_token"));
        }
        let device = asked["device_code"].as_str().unwrap_or_default();
        let polls = answering.polls.get_mut(device);
        return polls.and_then(Answering::next).unwrap_or_else(|| Answer::device_flow_error("incorrect_device_code"));
    }
    let request_line = head.lines().next().unwrap_or_default();
    if let Some(path) = request_line.strip_prefix("GET /")
        && let Some((repository, _)) = path.split_once(".git/info/refs?service=git-receive-pack ")
    {
        let answer = answering.pushes.get_mut(repository).and_then(Answering::next);
        return answer.unwrap_or_else(|| Answer::git_says(404, "Repository not found."));
    }
    let token = head.lines().find_map(|line| line.strip_prefix("authorization: Bearer ")).unwrap_or_default();
    answering.users.get(token).cloned().unwrap_or_else(|| Answer::status(401, Vec::new()))
}
