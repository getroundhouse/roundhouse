//! Reading workspaces' pull requests from GitHub, many at once as one token: first which pull requests each branch on
//! GitHub has, in one query for every workspace, then the one that's each workspace's, together with those its waits
//! watch, with the reader that reads their checks and stacks and confirms nothing moved while they were read.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    Branch, BranchName, GithubToken, PullRequestFound, PullRequestNumber, PullRequestsSnapshot, Workspace, WorkspaceId,
};
use protocol::worker::GitStatus;

use super::asking::{Asking, Plan, WorkspaceRead};
use super::judging::{Wanted, chosen, local};
use super::scopes::{Scope, WaitsRead, scopes};
use crate::hub::Hub;
use crate::hub::pull_requests::Asked;
use crate::hub::pull_requests::reading::{Found, couldnt_read};
use crate::jobs;
use github::{Headed, HeadsAsk, PullRequestsAsk, RateLimit, Unread};

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
    /// What its own comes to, when that's known without reading it, as for a branch with none.
    settled: Option<Result<Found, ApiError>>,
    /// The branch its own was found on, when it's read.
    on: Option<BranchName>,
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
        let mut readings: Vec<Reading> = plans.into_iter().map(|plan| reading(plan, &mut headed)).collect();
        let asks: Vec<PullRequestsAsk> =
            readings.iter().flat_map(|reading| reading.scopes.iter().map(|scope| scope.ask.clone())).collect();
        let answers = if asks.is_empty() {
            Vec::new()
        } else if let Some(error) = asked.leaving().stops_at(rate_limit) {
            asks.iter().map(|_| Err(Unread { error: error.clone(), affected: Vec::new() })).collect()
        } else {
            jobs::step(format!("waiting for GitHub to answer for {} pull requests", asks.len()));
            let read = self.github.read_pull_requests(token, asks, asked.leaving()).await;
            rate_limit = read.rate_limit.or(rate_limit);
            self.keep_read_summaries(read.summaries).await;
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
        for workspace in readings.iter().filter(|reading| reading.on.is_some()).map(|reading| &reading.workspace) {
            let running = self.workers.lock().get(&workspace.id).filter(|link| link.is_open()).cloned();
            let (Some(link), Some(path)) = (running, workspace.path.clone()) else { continue };
            jobs::step(format!("asking {link} how many files are uncommitted"));
            if let Ok(Ok(status)) = tokio::time::timeout(STATUS_WITHIN, link.call(GitStatus { path })).await {
                counted.insert(workspace.id.clone(), u32::try_from(status.files.len()).unwrap_or(u32::MAX));
            }
        }
        counted
    }
}

/// The branch on GitHub whose pull requests `plan` asks after, if it asks.
fn heads_of(plan: &Plan) -> Option<&HeadsAsk> {
    match &plan.own {
        Wanted::Heads { ask, .. } => Some(ask),
        Wanted::Nothing { .. } | Wanted::Unreadable { .. } => None,
    }
}

/// What reading `plan` asks of GitHub once `headed`, the answers to every plan's branch in turn, has said which pull
/// request is its own: that one, read where it merges into, which for a clone of a fork is the repository it was
/// forked from, together with what its waits watch.
fn reading(plan: Plan, headed: &mut impl Iterator<Item = Result<Vec<Headed>, ApiError>>) -> Reading {
    let Plan { workspace, own, waited } = plan;
    let (settled, own) = match own {
        Wanted::Nothing { on } => (Some(Ok(Found::nothing(on))), None),
        Wanted::Unreadable { error, .. } => (Some(Err(error)), None),
        Wanted::Heads { on, .. } => {
            let answer = headed.next().unwrap_or_else(|| Err(couldnt_read()));
            let known = workspace.known_pull_request().filter(|_| workspace.pull_request_branch.as_ref() == Some(&on));
            match answer
                .map(|headed| checked_out(&workspace).and_then(|branch| chosen(&headed, known, branch)).cloned())
            {
                Err(error) => (Some(Err(error)), None),
                Ok(None) => (Some(Ok(Found::nothing(Some(on)))), None),
                Ok(Some(Headed { into, number, .. })) => (None, Some((into, number, on))),
            }
        }
    };
    let (scopes, waits) = scopes(own.as_ref().map(|(into, number, _)| (into.clone(), *number)), waited);
    Reading { workspace, settled, on: own.map(|(_, _, on)| on), scopes, waits }
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
            if let (Some(number), Some(on)) = (scope.own, self.on.clone()) {
                own = Some(self.own_found(&answer, number, on, uncommitted));
            }
            if let Some(repository) = scope.waited {
                waits.push(WaitsRead { repository, read: answer });
            }
        }
        WorkspaceRead { own: own.unwrap_or_else(|| Err(couldnt_read())), waits }
    }

    /// The workspace's own pull request, `number`, found on `on`, as `answer` has it, with how its worktree stands
    /// against it.
    fn own_found(
        &self,
        answer: &Result<PullRequestsSnapshot, Unread>,
        number: PullRequestNumber,
        on: BranchName,
        uncommitted: &HashMap<WorkspaceId, u32>,
    ) -> Result<Found, ApiError> {
        let snapshot = answer.as_ref().map_err(|unread| unread.error.clone())?;
        let observed = snapshot.get(number).ok_or_else(|| ApiError::new(ErrorCode::Worker, "GitHub left it out"))?;
        let mut pull_request = observed.pull_request.clone();
        let workspace = &self.workspace;
        let known = workspace.known_pull_request().filter(|known| known.url == pull_request.url);
        let files = uncommitted.get(&workspace.id).copied().or(known.map(|known| known.local.uncommitted));
        if let Some(branch) = checked_out(workspace) {
            pull_request.local = local(&observed.head, branch, files.unwrap_or_default());
        }
        Ok(Found { found: PullRequestFound { pull_request: Some(pull_request) }, on: Some(on) })
    }
}

/// The branch the workspace has checked out, as its record says.
fn checked_out(workspace: &Workspace) -> Option<&Branch> {
    workspace.branches.as_ref().and_then(|branches| branches.checked_out())
}
