use self::naming::Try;
use super::super::ready::ReadyWorkspace;
use super::super::{Hub, new_id};
use super::naming;
use crate::error::internal;
use crate::store::{Made, now_ms};
use crate::worker_link::WorkerLink;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{
    AskKey, BranchName, ChildStatus, ChildWorktree, GitStatus, MadeBy, Repo, Rev, SessionId, WorkspaceId,
};
use protocol::worker;
use protocol::worker::{
    AddChildWorktree, ChildBranch, ChildWorktreeAdded, ListedChild, NewWorktreeOn, OwnWorktree, WatchChildBranches,
    WorktreeCreated, Worktrees,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::debug;

/// What each worktree an ask tries is made with.
pub(super) struct Making<'a> {
    pub(super) link: &'a WorkerLink,
    pub(super) repo: &'a Repo,
    /// The workspace's own worktree, which the others go beside.
    pub(super) own: &'a str,
    /// What a new branch starts from, or `None` for a branch that exists.
    pub(super) base: Option<String>,
}

impl Hub {
    /// Makes another worktree for the work of `session_id`'s agents, as a
    /// [`CreateWorktree`](protocol::worker::CreateWorktree) asks.
    pub(in super::super) async fn create_child(
        &self,
        workspace_id: &WorkspaceId,
        link: &WorkerLink,
        session_id: SessionId,
        on: NewWorktreeOn,
        key: AskKey,
    ) -> Result<WorktreeCreated, ApiError> {
        let _making = self.child_gate(workspace_id).read_owned().await;
        // Read with the gate held, which archiving takes once the record says it's under way, so a workspace on its
        // way out, or gone, is refused as anything that needs a ready one is.
        let ready = self.ready_workspace(workspace_id).await?;
        let (tries, base) = match &on {
            NewWorktreeOn::NewBranch { name, base } => {
                let tries = naming::new_branches(ready.workspace(), ready.path(), name);
                (tries, Some(base.clone().unwrap_or_else(|| "HEAD".to_string())))
            }
            NewWorktreeOn::Existing { branch } => (self.places_of_its(&ready, branch).await?, None),
        };
        let repo = self.repo_of(ready.workspace()).await?;
        let making = Making { link, repo: &repo, own: ready.path(), base };
        let (session, asked) = (session_id.clone(), key.clone());
        if let Some(earlier) = self.blocking(move |store| store.child_for_ask(&session, &asked)).await?
            && let Some(made) = self.answered_before(&making, earlier, &tries).await?
        {
            return Ok(made);
        }
        for Try { branch, path } in tries {
            let child = ChildWorktree {
                id: new_id().into(),
                workspace_id: workspace_id.clone(),
                path,
                branch,
                session_id: session_id.clone(),
                // Which of the chat's agents asked isn't told.
                made_by: MadeBy::Undetermined,
                created_at_ms: now_ms(),
                status: ChildStatus::Creating,
                branches: None,
                rev: Rev::default(),
            };
            let key = key.clone();
            let child = match self.blocking(move |store| store.insert_child(&child, &key)).await? {
                Some(Made::New(child)) => child,
                // Another of the workspace's worktrees is there.
                None => continue,
                Some(Made::Earlier(_)) => return Err(ApiError::new(ErrorCode::Conflict, "that's being made already")),
            };
            if let Some(made) = self.make(&making, &child).await? {
                return Ok(made);
            }
        }
        let taken = format!("every name tried in {} was taken", ready.workspace().name);
        Err(ApiError::new(ErrorCode::NameTaken, taken))
    }

    /// Where a worktree of `branch` goes, and the numbered places after it, or `NotFound` if it isn't one of the
    /// workspace's branches.
    pub(super) async fn places_of_its(
        &self,
        ready: &ReadyWorkspace,
        branch: &BranchName,
    ) -> Result<Vec<Try>, ApiError> {
        let id = ready.id().clone();
        let others = self.blocking(move |store| store.every_child_of(&id)).await?;
        naming::places_of_its(ready.workspace(), ready.path(), branch, &others).ok_or_else(|| {
            ApiError::new(ErrorCode::NotFound, format!("{branch} isn't one of this workspace's branches"))
        })
    }

    /// The answer for an ask heard again, from what it made the first time. One a hub stopped while making is made
    /// again where it was to be, which its worker answers with what it made there, once it's finished making it.
    /// `None` if that name was taken after all, which is forgotten, for the ask to try the next.
    pub(super) async fn answered_before(
        &self,
        making: &Making<'_>,
        earlier: ChildWorktree,
        tries: &[Try],
    ) -> Result<Option<WorktreeCreated>, ApiError> {
        match earlier.status {
            ChildStatus::Ready => {
                let status = git_status(making.link, &earlier.path).await;
                let commit =
                    status.and_then(|status| status.head).ok_or_else(|| internal("git couldn't say what it has"))?;
                Ok(Some(WorktreeCreated { worktree: earlier, commit, copied: vec![], warnings: vec![] }))
            }
            // Only if the ask still asks for it where it was to be. Otherwise the next worker to connect settles it.
            ChildStatus::Creating if tries.iter().any(|again| again.path == earlier.path) => {
                self.make(making, &earlier).await
            }
            ChildStatus::Creating => {
                Err(ApiError::new(ErrorCode::Conflict, format!("{} was being made", earlier.path)))
            }
            ChildStatus::Removed { .. } => {
                Err(ApiError::new(ErrorCode::Conflict, format!("{} was made, and removed since", earlier.path)))
            }
            ChildStatus::Unknown => Err(internal("a worktree's record says nothing this hub knows of where it is")),
        }
    }

    /// Has the worker make `child`, recorded as being made, and records it there. `None` if its name was taken, and
    /// then forgets it, as it does one the worker answers with any other error, since nothing was made. One whose
    /// worker went before answering stays recorded as being made, since the worker may make it anyway, and so does one
    /// the worker made that the hub then couldn't record: asking again finishes it, and the next worker to connect
    /// settles it.
    pub(super) async fn make(
        &self,
        making: &Making<'_>,
        child: &ChildWorktree,
    ) -> Result<Option<WorktreeCreated>, ApiError> {
        let added = match add(making, child).await {
            Ok(added) => added,
            Err(e) if e.code == ErrorCode::NameTaken => return self.discard(child).await.map(|()| None),
            Err(e) if !making.link.is_open() => return Err(e),
            Err(e) => return self.discard(child).await.and(Err(e)),
        };
        self.made(making.link, child, added).await.map(Some)
    }

    /// Records `child` there, as the worker `link` added it, with its branches as the worker read them, and has the
    /// worker watch them.
    pub(super) async fn made(
        &self,
        link: &WorkerLink,
        child: &ChildWorktree,
        added: ChildWorktreeAdded,
    ) -> Result<WorktreeCreated, ApiError> {
        let ChildWorktreeAdded { commit, copied, warnings, branches } = added;
        let id = child.id.clone();
        let made = self.blocking(move |store| store.child_ready(&id, branches.as_ref())).await?;
        let made = made.ok_or_else(|| ApiError::new(ErrorCode::Conflict, "it was removed as it was made"))?;
        watch_child(link, &made).await;
        Ok(WorktreeCreated { worktree: made, commit, copied, warnings })
    }

    /// Forgets a worktree its worker didn't make.
    pub(super) async fn discard(&self, child: &ChildWorktree) -> Result<(), ApiError> {
        let id = child.id.clone();
        self.blocking(move |store| store.discard_child(&id)).await
    }

    /// The workspace's gate for its other worktrees, which `Hub::child_gates` describes.
    pub(in crate::hub) fn child_gate(&self, workspace_id: &WorkspaceId) -> Arc<RwLock<()>> {
        self.child_gates.lock().entry(workspace_id.clone()).or_default().clone()
    }

    /// Lists the workspace's worktrees for an agent, as a [`ListWorktrees`](protocol::worker::ListWorktrees) asks,
    /// with what their git says now.
    pub(in super::super) async fn list_worktrees(
        &self,
        workspace_id: &WorkspaceId,
        link: &WorkerLink,
    ) -> Result<Worktrees, ApiError> {
        let ready = self.ready_workspace(workspace_id).await?;
        let id = ready.id().clone();
        let mut children = Vec::new();
        for worktree in self.blocking(move |store| store.children_of(&id)).await? {
            let status = git_status(link, &worktree.path).await;
            children.push(ListedChild { worktree, status });
        }
        let own = OwnWorktree { path: ready.path().to_string(), status: git_status(link, ready.path()).await };
        Ok(Worktrees { own, children })
    }
}

/// Asks the worker for `child` where its record says. What becomes of the record when the worker refuses is for
/// whoever asks.
pub(super) async fn add(making: &Making<'_>, child: &ChildWorktree) -> Result<ChildWorktreeAdded, ApiError> {
    let branch = child.branch.clone();
    let on = match &making.base {
        Some(base) => ChildBranch::New { branch, base: base.clone() },
        None => ChildBranch::Existing { branch },
    };
    let adding = AddChildWorktree {
        repo_path: making.repo.path.clone(),
        own: making.own.to_string(),
        path: child.path.clone(),
        on,
    };
    making.link.call(adding).await
}

/// Has the worker watch the branches of `child`, as it does the workspace's own.
pub(super) async fn watch_child(link: &WorkerLink, child: &ChildWorktree) {
    if let Err(e) = link.send(WatchChildBranches { path: child.path.clone() }).await {
        debug!(path = child.path, "asking the worker to watch the branches: {}", e.message);
    }
}

/// What git says of the worktree at `path`, or `None` if it couldn't say, as when it's gone.
pub(super) async fn git_status(link: &WorkerLink, path: &str) -> Option<GitStatus> {
    link.call(worker::GitStatus { path: path.to_owned() }).await.ok()
}
