//! A worker serving a workspace's worktrees, and what the tests ask of it and read back.

use std::path::Path;
use std::time::Duration;

use protocol::Build;
use protocol::client::ApiError;
use protocol::model::BranchName;
use protocol::worker::{
    AddChildWorktree, ChildBranch, ChildWorktreeAdded, RemoveWorkspace, RepoSource, WorkerFrame, WorkerResponse,
};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use crate::branch_watch::Whose;
use crate::branch_watch::tests::PATIENCE;
use crate::git::tests::{new_repo, prepare, run};
use crate::serve::{Outgoing, Worker};
use crate::{Config, CredentialHelper, Deleter};

pub(super) fn watch() -> AbortHandle {
    tokio::spawn(std::future::pending::<()>()).abort_handle()
}

/// Waits for the task to end, as it does once it has been stopped and the runtime has run it.
pub(super) async fn ends(watch: &AbortHandle, which: &str) {
    let failure = format!("the watch {which} was never stopped");
    waiting::task::until(Duration::from_secs(10), failure, || watch.is_finished()).await;
}

/// The next frame `wanted` takes, of what the worker sent its hub, skipping the rest.
pub(super) async fn next_frame<T>(
    frames: &mut mpsc::Receiver<Outgoing>,
    wanted: impl Fn(&WorkerFrame) -> Option<T>,
) -> T {
    let found = async {
        loop {
            let frame = frames.recv().await.expect("the worker stopped sending").frame;
            if let Some(found) = wanted(&frame) {
                return found;
            }
        }
    };
    tokio::time::timeout(PATIENCE, found).await.expect("no such frame came")
}

pub(super) fn config(dir: &Path) -> Config {
    std::fs::create_dir_all(dir.join("state")).unwrap();
    Config {
        build: Build::UNSTAMPED,
        workspaces_root: dir.join("ws"),
        state_dir: dir.join("state"),
        clones_dir: dir.join("clones"),
        home: None,
        path: None,
        claude_bin: None,
        codex_bin: None,
        antigravity_bin: None,
        scratch_dir: dir.to_path_buf(),
        deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
        // Never run here, since nothing these start asks git for a credential.
        credential_helper: CredentialHelper::new("/bin/false", ["--"]),
        machine: crate::testing::given(),
        refused: None,
        timers: crate::Timers::DEFAULT,
    }
}

/// A worker for the workspace `alder`, whose own worktree is made, in a repository with an ignored `.env` in its main
/// checkout.
pub(super) struct Served {
    pub(super) dir: tempfile::TempDir,
    pub(super) worker: Worker,
    pub(super) source: RepoSource,
    pub(super) own: String,
}

pub(super) async fn served() -> Served {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::write(repo.join(".gitignore"), "ignored/\n.env\n").unwrap();
    run(&repo, &["commit", "-qam", "ignore .env"]);
    std::fs::write(repo.join(".env"), "SECRET=1\n").unwrap();
    let worker = Worker::new(config(dir.path()));
    let own = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap().path;
    Served { worker, source: RepoSource::Checkout { path: repo.to_str().unwrap().to_string() }, own, dir }
}

impl Served {
    /// Where a hub puts the workspace's other worktree `try`.
    pub(super) fn child(&self) -> String {
        let children = Path::new(&self.own).with_file_name(".children");
        children.join("alder").join("try").to_str().unwrap().to_string()
    }

    /// The request for that worktree, on a new branch from what the workspace has checked out.
    pub(super) fn adding(&self) -> AddChildWorktree {
        let on = ChildBranch::New { branch: BranchName::from("alder/try"), base: "HEAD".into() };
        AddChildWorktree { source: self.source.clone(), own: self.own.clone(), path: self.child(), on }
    }

    /// The request to remove the worktree at `path`, the workspace's own or that other one.
    pub(super) fn removing(&self, path: &str, force: bool) -> RemoveWorkspace {
        let own = (path != self.own).then(|| self.own.clone());
        RemoveWorkspace { source: self.source.clone(), path: path.to_string(), force, own }
    }
}

pub(super) fn added(response: Result<WorkerResponse, ApiError>) -> ChildWorktreeAdded {
    match response {
        Ok(WorkerResponse::ChildWorktreeAdded(added)) => added,
        other => panic!("not made: {other:?}"),
    }
}

/// Commits in `worktree`, and waits for its watch to send the new commit, in the frame `whose` watch sends.
pub(super) async fn committed(frames: &mut mpsc::Receiver<Outgoing>, worktree: &str, whose: Whose) {
    run(Path::new(worktree), &["commit", "-q", "--allow-empty", "-m", "more"]);
    let tip = run(Path::new(worktree), &["rev-parse", "HEAD"]);
    next_frame(frames, |frame| {
        let branches = match (whose, frame) {
            (Whose::Own, WorkerFrame::Branches { branches }) => branches,
            (Whose::Child, WorkerFrame::ChildBranches { path, branches }) if path == worktree => branches,
            _ => return None,
        };
        branches.checked_out().filter(|branch| branch.tip.as_str() == tip).map(drop)
    })
    .await;
}
