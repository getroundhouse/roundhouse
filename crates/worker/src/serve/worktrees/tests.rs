use std::path::Path;
use std::sync::Arc;

use protocol::Build;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{BranchName, Head};
use protocol::worker::{
    AddChildWorktree, ChildBranch, ChildWorktreeAdded, RemoveWorkspace, Request as _, RestoreWorkspace, WatchBranches,
    WatchChildBranches, WorkerFrame, WorkerResponse, WorktreeThere, WorktreeThereReply,
};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

use super::{Watches, add_child, remove, restore, there, watch_branches};
use crate::branch_watch::Whose;
use crate::branch_watch::tests::PATIENCE;
use crate::git::tests::{new_repo, prepare, run};
use crate::serve::moves::tests::BOUND;
use crate::serve::{Worker, handle};
use crate::{Config, Deleter};

fn watch() -> AbortHandle {
    tokio::spawn(std::future::pending::<()>()).abort_handle()
}

/// Whether the task has ended, given the runtime a chance to end it.
async fn ended(watch: &AbortHandle) -> bool {
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    watch.is_finished()
}

/// A watch of a worktree replaces the earlier one of it and no other, and stopping one, as removing it does, leaves
/// the rest.
#[tokio::test]
async fn a_watch_replaces_only_its_own_worktree_s_and_stopping_one_leaves_the_rest() {
    let mut watches = Watches::default();
    let (own, first, again, other) = (watch(), watch(), watch(), watch());

    watches.start("/w".into(), Whose::Own, own.clone());
    watches.start("/c/a".into(), Whose::Child, first.clone());
    watches.start("/c/b".into(), Whose::Child, other.clone());
    watches.start("/c/a".into(), Whose::Child, again.clone());
    watches.stop(Path::new("/c/b"));

    assert!(ended(&first).await, "replaced");
    assert!(ended(&other).await, "stopped");
    assert!(!ended(&own).await && !ended(&again).await, "the rest go on");
    watches.stop_all();
    assert!(ended(&own).await && ended(&again).await);
}

/// The next frame `wanted` takes, of what the worker sent its hub, skipping the rest.
async fn next_frame<T>(frames: &mut mpsc::Receiver<Arc<WorkerFrame>>, wanted: impl Fn(&WorkerFrame) -> Option<T>) -> T {
    let found = async {
        loop {
            let frame = frames.recv().await.expect("the worker stopped sending");
            if let Some(found) = wanted(&frame) {
                return found;
            }
        }
    };
    tokio::time::timeout(PATIENCE, found).await.expect("no such frame came")
}

fn config(dir: &Path) -> Config {
    Config {
        build: Build::UNSTAMPED,
        workspaces_root: dir.join("ws"),
        state_dir: None,
        home: None,
        path: None,
        claude_bin: None,
        codex_bin: None,
        scratch_dir: dir.to_path_buf(),
        deleter: Deleter::new("/bin/rm", ["-rf", "--"]),
        refused: None,
    }
}

/// A worker for the workspace `alder`, whose own worktree is made, in a repository with an ignored `.env` in its main
/// checkout.
struct Served {
    dir: tempfile::TempDir,
    worker: Worker,
    repo_path: String,
    own: String,
}

async fn served() -> Served {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::write(repo.join(".gitignore"), "ignored/\n.env\n").unwrap();
    run(&repo, &["commit", "-qam", "ignore .env"]);
    std::fs::write(repo.join(".env"), "SECRET=1\n").unwrap();
    let worker = Worker::new(config(dir.path()));
    let own = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap().path;
    Served { worker, repo_path: repo.to_str().unwrap().to_string(), own, dir }
}

impl Served {
    /// Where a hub puts the workspace's other worktree `try`.
    fn child(&self) -> String {
        let children = Path::new(&self.own).with_file_name(".children");
        children.join("alder").join("try").to_str().unwrap().to_string()
    }

    /// The request for that worktree, on a new branch from what the workspace has checked out.
    fn adding(&self) -> AddChildWorktree {
        let on = ChildBranch::New { branch: BranchName::from("alder/try"), base: "HEAD".into() };
        AddChildWorktree { repo_path: self.repo_path.clone(), own: self.own.clone(), path: self.child(), on }
    }

    /// The request to remove the worktree at `path`, the workspace's own or that other one.
    fn removing(&self, path: &str, force: bool) -> RemoveWorkspace {
        let own = (path != self.own).then(|| self.own.clone());
        RemoveWorkspace { repo_path: self.repo_path.clone(), path: path.to_string(), force, own }
    }
}

fn added(response: Result<WorkerResponse, ApiError>) -> ChildWorktreeAdded {
    match response {
        Ok(WorkerResponse::ChildWorktreeAdded(added)) => added,
        other => panic!("not made: {other:?}"),
    }
}

/// Commits in `worktree`, and waits for its watch to send the new commit, in the frame `whose` watch sends.
async fn committed(frames: &mut mpsc::Receiver<Arc<WorkerFrame>>, worktree: &str, whose: Whose) {
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

/// A new hub asks again for what the last didn't hear back about, which may still be being made.
#[tokio::test]
async fn an_add_heard_again_waits_for_one_under_way_and_answers_with_what_it_made() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();
    // Held as an add under way holds it, so whether the next waits doesn't turn on which git finishes first.
    let under_way = state.moves.alone(Path::new(&path)).await;
    let heard_again = add_child(state, served.adding());
    tokio::pin!(heard_again);
    let early = tokio::time::timeout(BOUND, &mut heard_again).await;
    assert!(early.is_err() && !Path::new(&path).exists(), "it didn't wait for the one under way");
    drop(under_way);
    let first = added(heard_again.await);

    let (second, third) = tokio::join!(add_child(state, served.adding()), add_child(state, served.adding()));

    let (second, third) = (added(second), added(third));
    assert_eq!((&second.commit, &third.commit), (&first.commit, &first.commit));
    let warned = [&second.warnings, &third.warnings];
    assert!(warned.iter().all(|warnings| warnings.is_empty()), "each found it made, rather than failing: {warned:?}");
}

/// The ignored files setup copies are copied into a new one, and into one asked for again where it's missing them,
/// which leaves what's there as it is.
#[tokio::test]
async fn ignored_files_are_copied_into_a_new_one_and_again_only_where_they_re_missing() {
    let served = served().await;
    let state = &served.worker.state;
    let env = Path::new(&served.child()).join(".env");

    let first = added(add_child(state, served.adding()).await);
    let copied_at_first = std::fs::read_to_string(&env).unwrap();
    std::fs::write(&env, "SECRET=2\n").unwrap();
    let kept = added(add_child(state, served.adding()).await);
    std::fs::remove_file(&env).unwrap();
    let again = added(add_child(state, served.adding()).await);

    assert_eq!((first.copied, copied_at_first.as_str()), (vec![".env".to_string()], "SECRET=1\n"));
    assert_eq!(first.branches.unwrap().head, Head::Branch { name: BranchName::from("alder/try") });
    assert!(kept.copied.is_empty(), "what was there was left: {:?}", kept.copied);
    assert_eq!(again.copied, [".env"], "copied again once it was missing");
}

/// A removal heard while the worktree is being made, as after a new hub finds it recorded as being made, removes what
/// the add makes, rather than finding nothing and leaving it.
#[tokio::test]
async fn a_removal_waits_for_an_add_under_way() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();

    // Joined, so the add is polled first and takes the path before the removal looks.
    let (made, removed) = tokio::join!(add_child(state, served.adding()), remove(state, served.removing(&path, true)));

    added(made);
    assert!(matches!(removed, Ok(WorkerResponse::Ok)), "{removed:?}");
    assert!(!Path::new(&path).exists(), "removed once it was made");
}

/// A hub that finds a workspace an earlier one was archiving asks whether its worktree is there, and hears how the
/// removal the worker is still carrying out ended, rather than that it's there while it goes.
#[tokio::test]
async fn whether_a_worktree_is_there_is_answered_once_its_removal_has_ended() {
    let served = served().await;
    let state = &served.worker.state;
    let own = served.own.clone();

    // Joined, so the removal is polled first and has the worktree to itself before the question is asked.
    let (removed, there) =
        tokio::join!(remove(state, served.removing(&own, false)), there(state, WorktreeThere { path: own.clone() }));

    assert!(matches!(removed, Ok(WorkerResponse::Ok)), "{removed:?}");
    assert_eq!(there.unwrap(), WorkerResponse::WorktreeThere(WorktreeThereReply { there: false }));
}

/// Putting a worktree back takes its turn too, so a hub that finds a workspace an earlier one was restoring hears it's
/// there, rather than that it isn't while git is still making it.
#[tokio::test]
async fn whether_a_worktree_is_there_is_answered_once_it_s_been_put_back() {
    let served = served().await;
    let state = &served.worker.state;
    let own = served.own.clone();
    remove(state, served.removing(&own, false)).await.unwrap();
    let restoring =
        RestoreWorkspace { repo_path: served.repo_path.clone(), path: own.clone(), branch: "alder/start".into() };

    // Joined, so the restore is polled first and has the worktree to itself before the question is asked.
    let (restored, there) = tokio::join!(restore(state, restoring), there(state, WorktreeThere { path: own }));

    assert!(matches!(restored, Ok(WorkerResponse::Ok)), "{restored:?}");
    assert_eq!(there.unwrap(), WorkerResponse::WorktreeThere(WorktreeThereReply { there: true }));
}

/// A removal refused for the work it would lose leaves the worktree there, so its branches go on being watched.
#[tokio::test]
async fn a_removal_that_s_refused_leaves_the_worktree_watched() {
    let served = served().await;
    let state = &served.worker.state;
    let path = served.child();
    added(add_child(state, served.adding()).await);
    watch_branches(state, path.clone(), Whose::Child).await.unwrap();
    std::fs::write(Path::new(&path).join("uncommitted.txt"), "work\n").unwrap();

    let refused = remove(state, served.removing(&path, false)).await.unwrap_err();

    assert_eq!(refused.code, ErrorCode::NeedsForce);
    assert!(state.watches.lock().children.contains_key(Path::new(&path)), "its watch stopped");
}

/// Through what the worker does with each request: another worktree is made beside the workspace's own, its watch
/// sends frames naming it while the watch of the workspace's own goes on, and removing it stops its watch, and only
/// its.
#[tokio::test]
async fn a_child_worktree_is_made_watched_and_removed_through_the_worker_s_requests() {
    let served = served().await;
    let worker = &served.worker;
    let (sent, mut frames) = mpsc::channel(64);
    worker.state.outbox.attach(sent, false).await;
    let ask = |request| handle(&worker.state, &worker.forwarders, Ok(request));
    let (own, child) = (served.own.clone(), served.child());
    ask(WatchBranches { path: own.clone() }.into_request()).await.unwrap();

    added(ask(served.adding().into_request()).await);
    ask(WatchChildBranches { path: child.clone() }.into_request()).await.unwrap();
    let watched = next_frame(&mut frames, |frame| {
        let WorkerFrame::ChildBranches { path, branches } = frame else { return None };
        (*path == child).then(|| branches.head.clone())
    });
    assert_eq!(watched.await, Head::Branch { name: BranchName::from("alder/try") });
    committed(&mut frames, &child, Whose::Child).await;
    committed(&mut frames, &own, Whose::Own).await;

    ask(served.removing(&child, false).into_request()).await.unwrap();

    assert!(!Path::new(&child).exists());
    let moved_to = served.dir.path().join("ws").join("repo").join(".deleting");
    assert!(moved_to.exists(), "moved aside beside the workspaces, which every archive clears");
    let left = worker.state.watches.lock();
    assert!(left.children.is_empty(), "its watch stopped");
    assert!(left.own.is_some(), "the workspace's own goes on");
}
