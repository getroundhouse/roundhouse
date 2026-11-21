//! A read of many asks at once, as one token: each round asks GitHub for what every ask needs next in as few queries
//! as GitHub's limits on a query allow, so asking of more pull requests costs more rounds only where they need more,
//! not a query each.

use protocol::client::{ApiError, ErrorCode};
use protocol::model::GithubToken;
use serde_json::Value;

use super::asked::{PullRequestsAsk, PullRequestsRead, RulesRead, SummaryRead, Unread};
use super::graphql::NoAnswer;
use super::query::{self, Asked, Part};
use super::reading::Reading;
use super::said::RateLimit;
use super::said::{failed, incomplete, usage};
use crate::Github;

/// The parts one query asks for at most. Each pull request's part asks for up to a hundred checks and a hundred of its
/// stack, and GitHub counts a query's points, and its time, by how much it could bring, so fifty keep one query to a
/// few points and well inside the ten seconds GitHub gives one, and a round of a hundred workspaces' pull requests
/// is still only a few queries.
pub(super) const PARTS_A_QUERY: usize = 50;

/// How many times an ask whose pull requests moved while it was read is read again from the start, all told, before
/// it fails. Two queries read most asks, a second or so apart, so one that moves twice in a row is moving all the time.
const ATTEMPTS: u32 = 3;

/// The rounds a read takes at most, which only a GitHub that pages for ever would reach.
const ROUNDS_AT_MOST: u32 = 30;

/// The part of GitHub's limit a read leaves alone, for agents and the person, whose own `gh` shares it, as the hub's
/// reads nobody is waiting on do too.
const KEPT_FOR_OTHERS_PERCENT: u32 = 20;

/// What a read leaves of GitHub's limit for the token, once what's left is down to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Leaving {
    /// [`KEPT_FOR_OTHERS_PERCENT`] of it, for agents and the person, whose own `gh` shares it.
    ForOthers,
    /// Nothing, for a read the person is waiting on, which is the reason the rest is kept.
    Nothing,
}

impl Leaving {
    /// How many of `limit`'s points it leaves.
    pub fn kept(self, limit: &RateLimit) -> u32 {
        match self {
            Self::ForOthers => limit.limit.saturating_mul(KEPT_FOR_OTHERS_PERCENT) / 100,
            Self::Nothing => 0,
        }
    }

    /// Why no more is asked, once what GitHub last said is left, `limit`, is down to what this leaves.
    pub fn stops_at(self, limit: Option<RateLimit>) -> Option<ApiError> {
        let limit = limit?;
        (limit.remaining <= self.kept(&limit)).then(|| {
            ApiError::new(
                ErrorCode::Worker,
                "GitHub's limit for reading pull requests is nearly used up until it resets",
            )
        })
    }
}

impl Github {
    /// Reads each of `asks` as `token`, together, each to its own answer, so one that fails, such as for a pull
    /// request that isn't there, fails no other. Stops asking once GitHub's limit for the token is down to what
    /// `leaving` leaves.
    pub async fn read_pull_requests(
        &self,
        token: &GithubToken,
        asks: Vec<PullRequestsAsk>,
        leaving: Leaving,
    ) -> PullRequestsRead {
        let mut spent = Spent::new(leaving);
        let mut answers: Vec<Option<Result<_, Unread>>> = asks.iter().map(|_| None).collect();
        let (mut summaries, mut rules) = (Vec::new(), Vec::new());
        let mut reading: Vec<(usize, Reading)> = asks.iter().cloned().map(Reading::new).enumerate().collect();
        for attempt in 1..=ATTEMPTS {
            self.read_rounds(token, &mut reading, &mut spent).await;
            for (index, mut read) in std::mem::take(&mut reading) {
                let read_summaries = read.take_summaries();
                let read_rules = read.read_rules();
                match read.finish() {
                    // What it found of them moved as it was read, so the next attempt reads them again.
                    Err(unread) if unread.error.code == ErrorCode::Conflict && attempt < ATTEMPTS => {
                        reading.push((index, Reading::new(asks[index].clone())));
                    }
                    answer => {
                        let repository = &asks[index].repository;
                        summaries.extend(read_summaries.into_iter().map(|(number, summary)| SummaryRead {
                            repository: repository.clone(),
                            number,
                            summary,
                        }));
                        rules.extend(read_rules.into_iter().map(|(branch, read)| RulesRead {
                            repository: repository.clone(),
                            branch,
                            rules: read,
                        }));
                        answers[index] = Some(answer);
                    }
                }
            }
            if reading.is_empty() {
                break;
            }
        }
        let unanswered = || Err(Unread { error: incomplete(), affected: Vec::new() });
        let answers = answers.into_iter().map(|answer| answer.unwrap_or_else(unanswered)).collect();
        PullRequestsRead { answers, summaries, rules, rate_limit: spent.rate_limit, refused: spent.refused }
    }

    /// Asks for what each of `reading` needs, round after round, until none needs more.
    async fn read_rounds(&self, token: &GithubToken, reading: &mut [(usize, Reading)], spent: &mut Spent) {
        for _ in 0..ROUNDS_AT_MOST {
            let wanted: Vec<(usize, Vec<Part>)> =
                reading.iter().enumerate().map(|(at, (_, read))| (at, read.parts())).collect();
            let batches = batches(wanted.into_iter().filter(|(_, parts)| !parts.is_empty()).collect());
            if batches.is_empty() {
                return;
            }
            for batch in batches {
                match self.spend(token, &written(reading, &batch), spent).await {
                    Ok(answer) => take(&answer, &batch, reading),
                    Err(error) => {
                        for (at, _) in &batch {
                            reading[*at].1.fail(error.clone(), None);
                        }
                    }
                }
            }
        }
        for (_, read) in reading.iter_mut() {
            read.fail(incomplete(), None);
        }
    }
}

/// The query that asks for `batch`.
fn written(reading: &[(usize, Reading)], batch: &[(usize, Vec<Part>)]) -> String {
    let asked: Vec<Asked<'_>> = batch
        .iter()
        .map(|(at, parts)| Asked { index: *at, repository: &reading[*at].1.ask.repository, parts: parts.clone() })
        .collect();
    query::query(&asked)
}

/// `wanted` in batches of at most [`PARTS_A_QUERY`] parts, an ask's parts split between batches where they don't fit.
pub(super) fn batches(wanted: Vec<(usize, Vec<Part>)>) -> Vec<Vec<(usize, Vec<Part>)>> {
    let mut batches: Vec<Vec<(usize, Vec<Part>)>> = Vec::new();
    let mut batch: Vec<(usize, Vec<Part>)> = Vec::new();
    let mut room = PARTS_A_QUERY;
    for (at, parts) in wanted {
        for part in parts {
            if room == 0 {
                batches.push(std::mem::take(&mut batch));
                room = PARTS_A_QUERY;
            }
            match batch.last_mut() {
                Some((last, parts)) if *last == at => parts.push(part),
                _ => batch.push((at, vec![part])),
            }
            room -= 1;
        }
    }
    if !batch.is_empty() {
        batches.push(batch);
    }
    batches
}

/// Hands each ask in `batch` its parts of `answer`, and the errors GitHub gave about them, by the aliases their
/// paths start with. An error whose path names no ask's part, such as one about the query itself, fails them all.
fn take(answer: &Value, batch: &[(usize, Vec<Part>)], reading: &mut [(usize, Reading)]) {
    for error in answer["errors"].as_array().into_iter().flatten() {
        let path: Vec<&str> = error["path"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let asked = path.first().and_then(|alias| alias.strip_prefix('r')).and_then(|at| at.parse::<usize>().ok());
        match asked.and_then(|at| batch.iter().find(|(asked, _)| *asked == at)) {
            Some((at, parts)) => {
                let part = path.get(1).and_then(|alias| parts.iter().find(|part| part.alias() == *alias));
                reading[*at].1.fail(failed(error), part.and_then(Part::about));
            }
            None => batch.iter().for_each(|(at, _)| reading[*at].1.fail(failed(error), None)),
        }
    }
    for (at, parts) in batch {
        let repository = &answer["data"][format!("r{at}")];
        let read = &mut reading[*at].1;
        read.take_repository(repository);
        for part in parts {
            read.take(part, &repository[part.alias()]);
        }
    }
}

/// What a read has spent, and what GitHub last said is left.
pub(crate) struct Spent {
    pub(crate) rate_limit: Option<RateLimit>,
    pub(crate) refused: bool,
    leaving: Leaving,
}

impl Github {
    /// What GitHub answers `query` with, as `token`, unless what's left of its limit is down to what `spent` leaves,
    /// in which case it isn't asked. Takes into `spent` what GitHub says is left, and whether it refused the token.
    pub(crate) async fn spend(&self, token: &GithubToken, query: &str, spent: &mut Spent) -> Result<Value, ApiError> {
        if let Some(exhausted) = spent.exhausted() {
            return Err(exhausted);
        }
        let answered = self.graphql(token, query).await;
        spent.took(answered.as_ref().ok());
        spent.refused |= matches!(answered, Err(NoAnswer::Refused));
        answered.map_err(|no| no.error())
    }
}

impl Spent {
    /// What a read that leaves `leaving` of GitHub's limit has spent before it asks anything.
    pub(crate) fn new(leaving: Leaving) -> Self {
        Self { rate_limit: None, refused: false, leaving }
    }

    /// Why no more is asked, once what's left of GitHub's limit is down to what the read leaves.
    fn exhausted(&self) -> Option<ApiError> {
        self.leaving.stops_at(self.rate_limit)
    }

    /// Takes in what GitHub said is left of its limit after a query, if it said.
    fn took(&mut self, answer: Option<&Value>) {
        if let Some(limit) = answer.and_then(usage) {
            self.rate_limit = Some(limit);
        }
    }
}
