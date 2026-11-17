//! Reading workspaces' own pull requests from GitHub, many at once as one token: first which pull requests each
//! branch on GitHub has, in one query for every workspace, then the one that's each workspace's, with the reader the
//! waits use, which reads its checks and confirms nothing moved while it was read.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    Branch, BranchName, GithubToken, PullRequestFound, PullRequestSelection, Workspace, WorkspaceId,
};
use protocol::worker::GitStatus;

use super::judging::{Wanted, chosen, local, wanted};
use crate::github::{Headed, HeadsAsk, PullRequestsAsk};
use crate::hub::Hub;
use crate::hub::pull_requests::Asked;
use crate::hub::pull_requests::reading::Found;
use crate::jobs;

/// How long a read waits for a worker that's running already to say how many files are uncommitted, which it reads
/// without asking GitHub anything.
const STATUS_WITHIN: Duration = Duration::from_secs(10);

/// What reading workspaces' pull requests found, and what GitHub last said of its limit for the token.
pub(in crate::hub::pull_requests) struct OwnRead {
    pub(in crate::hub::pull_requests) found: Vec<(WorkspaceId, Result<Found, ApiError>)>,
    pub(in crate::hub::pull_requests) rate_limit: Option<protocol::worker::RateLimit>,
    pub(in crate::hub::pull_requests) refused: bool,
}

/// What reading workspaces' pull requests needs of GitHub, from their records: those that need nothing, as one whose
/// branch is pushed nowhere, with what they come to, and the branch on GitHub of each of the rest.
pub(in crate::hub::pull_requests) struct Asking {
    settled: Vec<(WorkspaceId, Result<Found, ApiError>)>,
    asked: Vec<(Workspace, HeadsAsk, BranchName)>,
}

impl Asking {
    /// The first of them that needs GitHub, whose worker is asked for a machine's token if one is needed.
    pub(in crate::hub::pull_requests) fn first_asked(&self) -> Option<WorkspaceId> {
        self.asked.first().map(|(workspace, _, _)| workspace.id.clone())
    }

    /// Whether any of them needs GitHub, and so a token.
    pub(in crate::hub::pull_requests) fn needs_github(&self) -> bool {
        !self.asked.is_empty()
    }

    /// What they come to with nothing read, for when none needs GitHub, or else with those that do unread.
    pub(in crate::hub::pull_requests) fn settled(self) -> OwnRead {
        self.failed(&super::super::reading::couldnt_read())
    }

    /// The same, where those that need GitHub can't be read, as `error` says, such as with no token to read them as.
    pub(in crate::hub::pull_requests) fn failed(self, error: &ApiError) -> OwnRead {
        let mut found = self.settled;
        found.extend(self.asked.into_iter().map(|(workspace, _, _)| (workspace.id, Err(error.clone()))));
        OwnRead { found, rate_limit: None, refused: false }
    }
}

impl Hub {
    /// What reading the pull request of each of `workspaces` needs of GitHub, as [`wanted`] says.
    pub(in crate::hub::pull_requests) async fn asking(&self, workspaces: Vec<Workspace>) -> Asking {
        let (mut settled, mut asked) = (Vec::new(), Vec::new());
        for workspace in workspaces {
            let address = self.repo_of(&workspace).await.ok().and_then(|repo| repo.origin.url().cloned());
            match wanted(&workspace, address.as_ref()) {
                Wanted::Nothing { on } => settled.push((workspace.id.clone(), Ok(Found { found: none(), on }))),
                Wanted::Unreadable { error, .. } => settled.push((workspace.id.clone(), Err(error))),
                Wanted::Heads { on, ask } => asked.push((workspace, ask, on)),
            }
        }
        Asking { settled, asked }
    }

    /// Reads the pull request of each of `asking`'s workspaces as `token`, together, each to its own answer, so one
    /// that can't be read holds up no other, for `asked`, leaving what it leaves of GitHub's limit. For one someone
    /// asked for, asks the worker of each that's running already how many files are uncommitted, and keeps what was
    /// known for the rest, so a read never starts or keeps a worker.
    pub(in crate::hub::pull_requests) async fn read_own(
        self: &Arc<Self>,
        token: &GithubToken,
        asking: Asking,
        asked: Asked,
    ) -> OwnRead {
        let Asking { settled: mut found, asked: heads_of } = asking;
        if heads_of.is_empty() {
            return OwnRead { found, rate_limit: None, refused: false };
        }
        jobs::step(format!("waiting for GitHub to say which pull requests {} branches have", heads_of.len()));
        let heads: Vec<HeadsAsk> = heads_of.iter().map(|(_, ask, _)| ask.clone()).collect();
        let headed = self.github.read_heads(token, &heads, asked.leaving()).await;
        let (mut rate_limit, mut refused) = (headed.rate_limit, headed.refused);
        let mut reading: Vec<(Workspace, PullRequestsAsk, BranchName)> = Vec::new();
        for ((workspace, _, on), answer) in heads_of.into_iter().zip(headed.answers) {
            let known = workspace.known_pull_request().filter(|_| workspace.pull_request_branch.as_ref() == Some(&on));
            let theirs =
                answer.map(|headed| checked_out(&workspace).and_then(|branch| chosen(&headed, known, branch)).cloned());
            match theirs {
                Err(error) => found.push((workspace.id.clone(), Err(error))),
                Ok(None) => found.push((workspace.id.clone(), Ok(Found { found: none(), on: Some(on) }))),
                // Read where it merges into, which for a clone of a fork is the repository it was forked from.
                Ok(Some(Headed { into, number, .. })) => {
                    let selection = PullRequestSelection::from(number);
                    reading.push((workspace, PullRequestsAsk { repository: into, selection }, on));
                }
            }
        }
        if reading.is_empty() {
            return OwnRead { found, rate_limit, refused };
        }
        if let Some(error) = asked.leaving().stops_at(rate_limit) {
            found.extend(reading.into_iter().map(|(workspace, _, _)| (workspace.id, Err(error.clone()))));
            return OwnRead { found, rate_limit, refused };
        }
        jobs::step(format!("waiting for GitHub to answer for {} pull requests", reading.len()));
        let asks: Vec<PullRequestsAsk> = reading.iter().map(|(_, ask, _)| ask.clone()).collect();
        let read = self.github.read_pull_requests(token, asks, asked.leaving()).await;
        rate_limit = read.rate_limit.or(rate_limit);
        refused |= read.refused;
        let uncommitted = if asked == Asked::InTurn { HashMap::new() } else { self.uncommitted(&reading).await };
        for ((workspace, ask, on), answer) in reading.into_iter().zip(read.answers) {
            let number = ask.selection.numbers()[0];
            let pull_request = answer.map_err(|unread| unread.error).and_then(|snapshot| {
                let observed =
                    snapshot.get(number).ok_or_else(|| ApiError::new(ErrorCode::Worker, "GitHub left it out"))?;
                let mut pull_request = observed.pull_request.clone();
                let known = workspace.known_pull_request().filter(|known| known.url == pull_request.url);
                let files = uncommitted.get(&workspace.id).copied().or(known.map(|known| known.local.uncommitted));
                if let Some(branch) = checked_out(&workspace) {
                    pull_request.local = local(&observed.head, branch, files.unwrap_or_default());
                }
                Ok(pull_request)
            });
            let pull_request =
                pull_request.map(|pr| Found { found: PullRequestFound { pull_request: Some(pr) }, on: Some(on) });
            found.push((workspace.id.clone(), pull_request));
        }
        OwnRead { found, rate_limit, refused }
    }

    /// How many files are uncommitted in each of `reading`'s worktrees whose worker is running already, as it says.
    async fn uncommitted(&self, reading: &[(Workspace, PullRequestsAsk, BranchName)]) -> HashMap<WorkspaceId, u32> {
        let mut counted = HashMap::new();
        for (workspace, _, _) in reading {
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

/// The branch the workspace has checked out, as its record says.
fn checked_out(workspace: &Workspace) -> Option<&Branch> {
    workspace.branches.as_ref().and_then(|branches| branches.checked_out())
}

fn none() -> PullRequestFound {
    PullRequestFound { pull_request: None }
}
