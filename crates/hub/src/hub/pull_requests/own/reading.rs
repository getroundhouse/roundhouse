//! Reading workspaces' pull requests from GitHub, many at once as one token: first which pull requests each branch on
//! GitHub has, in one query for every workspace, then the one that's each workspace's, together with those its waits
//! watch, with the reader that reads their checks and stacks and confirms nothing moved while they were read.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    Branch, BranchName, GithubRepositoryName, GithubToken, Local, OwnPullRequest, PullRequestNumber,
    PullRequestsSnapshot, Workspace, WorkspaceId,
};
use protocol::worker::GitStatus;

use super::asking::{Asking, Plan, WorkspaceRead};
use super::judging::{Wanted, chosen, local};
use super::scopes::{Scope, WaitsRead, scopes};
use crate::hub::Hub;
use crate::hub::pull_requests::Asked;
use crate::hub::pull_requests::reading::couldnt_read;
use crate::hub::pull_requests::recording::Found;
use crate::hub::pull_requests::unseen::OwnUnread;
use crate::jobs;
use github::{Headed, HeadsAsk, HeadsUnread, PullRequestsAsk, RateLimit, Unread};

/// How long a read waits for a worker that's running already to say how many files are uncommitted, which it reads
/// without asking GitHub anything.
const STATUS_WITHIN: Duration = Duration::from_secs(10);

/// What reading workspaces' pull requests found, and what GitHub last said of its limit for the token.
pub(in crate::hub::pull_requests) struct OwnRead {
    pub(in crate::hub::pull_requests) found: Vec<(WorkspaceId, WorkspaceRead)>,
    pub(in crate::hub::pull_requests) rate_limit: Option<RateLimit>,
}

/// One workspace's read, once GitHub has said which pull request is its own.
struct Reading {
    workspace: Workspace,
    /// The branch its worktree has checked out, as its branches said as the read was planned.
    checked_out: Option<Branch>,
    /// How many files were uncommitted as its pull request was last read, and that pull request's address, for one
    /// whose worker doesn't say now.
    known_uncommitted: Option<(String, u32)>,
    /// What its own comes to, when that's known without reading it, as for a branch with none.
    settled: Option<Result<Found, OwnUnread>>,
    /// The repository its own merges into, and the branch it was found on, when it's read.
    own: Option<(GithubRepositoryName, BranchName)>,
    scopes: Vec<Scope>,
    waits: Vec<WaitsRead>,
}

impl Hub {
    /// Reads the pull requests of each of `asking`'s workspaces as `token`, together, each to its own answer, so one
    /// that can't be read holds up no other, for `asked`, leaving what it leaves of GitHub's limit. For one someone
    /// asked for, asks the worker of each that's running already how many files are uncommitted, and keeps what was
    /// known for the rest, so a read never starts or keeps a worker.
    pub(in crate::hub::pull_requests) async fn read_own(
        self: &Arc<Self>,
        token: &GithubToken,
        asking: Asking,
        asked: Asked,
    ) -> OwnRead {
        let Asking(plans) = asking;
        let heads: Vec<HeadsAsk> = plans.iter().filter_map(|plan| heads_of(plan).cloned()).collect();
        let (mut rate_limit, mut headed) = (None, Vec::new().into_iter());
        if !heads.is_empty() {
            jobs::step(format!("waiting for GitHub to say which pull requests {} branches have", heads.len()));
            let read = self.github.read_heads(token, &heads, asked.leaving()).await;
            (rate_limit, headed) = (read.rate_limit, read.answers.into_iter());
        }
        let mut readings: Vec<Reading> = plans
            .into_iter()
            .map(|plan| {
                let known = plan.workspace.pull_request_linked().and_then(|link| {
                    self.uncommitted_known(&plan.workspace.id, &link.url).map(|files| (link.url.clone(), files))
                });
                reading(plan, known, &mut headed)
            })
            .collect();
        let asks: Vec<PullRequestsAsk> =
            readings.iter().flat_map(|reading| reading.scopes.iter().map(|scope| scope.ask.clone())).collect();
        let answers = if asks.is_empty() {
            Vec::new()
        } else if let Some(error) = asked.leaving().stops_at(rate_limit) {
            asks.iter().map(|_| Err(Unread { error: error.clone(), affected: Vec::new() })).collect()
        } else {
            jobs::step(format!("waiting for GitHub to answer for {} pull requests", asks.len()));
            let read = self.read_pull_requests(token, asks, asked.leaving()).await;
            rate_limit = read.rate_limit.or(rate_limit);
            read.answers
        };
        let uncommitted = if asked == Asked::InTurn { HashMap::new() } else { self.uncommitted(&readings).await };
        let mut answers = answers.into_iter();
        let found = readings
            .iter_mut()
            .map(|reading| (reading.workspace.id.clone(), reading.answered(&mut answers, &uncommitted)))
            .collect();
        OwnRead { found, rate_limit }
    }

    /// How many files are uncommitted in the worktree of each of `readings` whose own pull request is read and whose
    /// worker is running already, as it says.
    async fn uncommitted(&self, readings: &[Reading]) -> HashMap<WorkspaceId, u32> {
        let mut counted = HashMap::new();
        for workspace in readings.iter().filter(|reading| reading.own.is_some()).map(|reading| &reading.workspace) {
            if let Some(files) = self.uncommitted_in(workspace).await {
                counted.insert(workspace.id.clone(), files);
            }
        }
        counted
    }

    /// How many files are uncommitted in `workspace`'s worktree, if its worker is running already and says.
    pub(in crate::hub::pull_requests) async fn uncommitted_in(&self, workspace: &Workspace) -> Option<u32> {
        let running = self.workers.lock().get(&workspace.id).filter(|link| link.is_open()).cloned();
        let (link, path) = (running?, workspace.path.clone()?);
        jobs::step(format!("asking {link} how many files are uncommitted"));
        let status = tokio::time::timeout(STATUS_WITHIN, link.call(GitStatus { path })).await.ok()?.ok()?;
        Some(u32::try_from(status.files.len()).unwrap_or(u32::MAX))
    }
}

/// The branch on GitHub whose pull requests `plan` asks after, if it asks.
fn heads_of(plan: &Plan) -> Option<&HeadsAsk> {
    match &plan.own {
        Wanted::Heads { ask, .. } => Some(ask),
        Wanted::Nothing { .. } | Wanted::NotKnown | Wanted::Unreadable { .. } => None,
    }
}

/// What reading `plan` asks of GitHub once `headed`, the answers to every plan's branch in turn, has said which pull
/// request is its own: that one, read where it merges into, which for a clone of a fork is the repository it was
/// forked from, together with what its waits watch. `known_uncommitted` is how many files were uncommitted as the
/// pull request the workspace had was last read, and its address.
fn reading(
    plan: Plan,
    known_uncommitted: Option<(String, u32)>,
    headed: &mut impl Iterator<Item = Result<Vec<Headed>, HeadsUnread>>,
) -> Reading {
    let Plan { workspace, checked_out, own, waited } = plan;
    let (settled, own) = match own {
        Wanted::Nothing { on } => (Some(Ok(Found::nothing(on))), None),
        Wanted::NotKnown => (Some(Err(OwnUnread::NotKnown)), None),
        Wanted::Unreadable { error, .. } => (Some(Err(error.into())), None),
        Wanted::Heads { ask, on } => {
            let answer =
                headed.next().unwrap_or_else(|| Err(HeadsUnread::Failed(couldnt_read()))).map_err(
                    |unread| match unread {
                        HeadsUnread::Unseen { said } => OwnUnread::Unseen { repository: ask.repository.clone(), said },
                        HeadsUnread::Failed(error) => OwnUnread::Failed(error),
                    },
                );
            let known = (workspace.pull_request_linked())
                .filter(|_| workspace.pull_request_branch.as_ref() == Some(&on))
                .map(|link| link.url.as_str());
            match answer.map(|headed| checked_out.as_ref().and_then(|branch| chosen(&headed, known, branch)).cloned()) {
                Err(error) => (Some(Err(error)), None),
                Ok(None) => (Some(Ok(Found::nothing(Some(on)))), None),
                Ok(Some(Headed { into, number, .. })) => (None, Some((into, number, on))),
            }
        }
    };
    let (scopes, waits) = scopes(own.as_ref().map(|(into, number, _)| (into.clone(), *number)), waited);
    let own = own.map(|(into, _, on)| (into, on));
    Reading { workspace, checked_out, known_uncommitted, settled, own, scopes, waits }
}

impl Reading {
    /// What it found, from `answers`, the answers to every reading's asks in turn, and `uncommitted`, how many files
    /// each worktree whose worker said has uncommitted.
    fn answered(
        &mut self,
        answers: &mut impl Iterator<Item = Result<PullRequestsSnapshot, Unread>>,
        uncommitted: &HashMap<WorkspaceId, u32>,
    ) -> WorkspaceRead {
        let mut own = self.settled.take();
        let mut waits = std::mem::take(&mut self.waits);
        for scope in std::mem::take(&mut self.scopes) {
            let answer = answers.next().unwrap_or_else(|| Err(Unread { error: couldnt_read(), affected: Vec::new() }));
            if let (Some(number), Some((into, on))) = (scope.own, self.own.clone()) {
                own = Some(self.own_found(&answer, number, into, on, uncommitted).map_err(OwnUnread::from));
            }
            if let Some(repository) = scope.waited {
                waits.push(WaitsRead { repository, read: answer });
            }
        }
        WorkspaceRead { own: own.unwrap_or_else(|| Err(couldnt_read().into())), waits }
    }

    /// The workspace's own pull request, `number` of `into`, found on `on`, as `answer` has it, with how its worktree
    /// stands against it.
    fn own_found(
        &self,
        answer: &Result<PullRequestsSnapshot, Unread>,
        number: PullRequestNumber,
        into: GithubRepositoryName,
        on: BranchName,
        uncommitted: &HashMap<WorkspaceId, u32>,
    ) -> Result<Found, ApiError> {
        let snapshot = answer.as_ref().map_err(|unread| unread.error.clone())?;
        let pull_request =
            snapshot.get(number).ok_or_else(|| ApiError::new(ErrorCode::Worker, "GitHub left it out"))?.clone();
        let url = &pull_request.said().about.url;
        let known = self.known_uncommitted.as_ref().filter(|(known, _)| known == url).map(|(_, files)| *files);
        let files = uncommitted.get(&self.workspace.id).copied().or(known);
        let local = match &self.checked_out {
            Some(branch) => local(&pull_request.said().head, branch, files.unwrap_or_default()),
            None => Local::default(),
        };
        Ok(Found::of(OwnPullRequest { pull_request, local }, into, Some(on)))
    }
}
