use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use protocol::client::{ApiError, ErrorCode};
use protocol::fixtures;
use protocol::model::{
    AskKey, Branch, BranchName, Branches, ChildStatus, ChildWorktree, GitFile, GitStatus, Head, MadeBy, WorkName,
};
use protocol::worker::{
    self, Answer, Ask, ChildBranch, ChildWorktreeAdded, CreateWorktree, ListWorktrees, NewWorktreeOn, RemoveWorktree,
    WorkerRequest, WorkerResponse, WorktreeCreated, WorktreeRemoval, WorktreeRemoved, WorktreeThereReply, Worktrees,
};

use crate::hub::Hub;
use crate::hub::tests::hub_in;
use crate::store::Move;
use crate::worker_link::WorkerLink;

mod gates;
mod removing;
mod settling;

/// Where the workspace's other worktree `work` goes.
fn at(work: &str) -> String {
    format!("/ws/.children/alder/{work}")
}

/// What a stand-in worker was asked to do with worktrees.
#[derive(Default)]
struct Seen {
    /// How many requests of every kind.
    calls: usize,
    /// Each worktree it was asked to add, by its path.
    added: Vec<(String, ChildBranch)>,
    removed: Vec<String>,
    watched: Vec<String>,
}

/// How a stand-in worker answers. Left as it is, it makes each worktree it's asked for, removes each, and says each is
/// there with nothing to lose.
#[derive(Clone, Copy, Default)]
struct StandIn {
    /// The paths it refuses to make a worktree at, as taken.
    taken: &'static [&'static str],
    /// The paths whose worktrees have gone: git can't say anything of them, and they aren't there.
    gone: &'static [&'static str],
    /// Whether each of the workspace's other worktrees has commits on no branch.
    others_lose: bool,
    /// Whether the workspace's own worktree has a change that isn't committed.
    own_loses: bool,
    /// A path whose worktree it refuses to remove unless forced, as one an agent wrote in after the hub looked.
    refuses_to_remove: Option<&'static str>,
    /// A path whose worktree it can't remove even forced, as one that's no longer a worktree.
    cant_remove: Option<&'static str>,
    /// A path it can't make a worktree at, for a reason that isn't its name.
    cant_make: Option<&'static str>,
}

impl StandIn {
    fn answer(self, seen: &Mutex<Seen>, request: WorkerRequest) -> Result<WorkerResponse, ApiError> {
        seen.lock().calls += 1;
        if let WorkerRequest::AddChildWorktree(adding) = request {
            seen.lock().added.push((adding.path.clone(), adding.on));
            if self.taken.contains(&adding.path.as_str()) {
                return Err(ApiError::new(ErrorCode::NameTaken, "taken"));
            }
            if self.cant_make == Some(adding.path.as_str()) {
                return Err(ApiError::new(ErrorCode::Conflict, "checked out elsewhere"));
            }
            let (commit, copied) = ("abc".into(), vec![".env".into()]);
            let added = ChildWorktreeAdded { commit, copied, warnings: vec![], branches: None };
            return Ok(WorkerResponse::ChildWorktreeAdded(added));
        }
        if let WorkerRequest::GitStatus(worker::GitStatus { path }) = &request {
            return self.status(path).map(WorkerResponse::GitStatus);
        }
        if let WorkerRequest::WorktreeThere(asked) = &request {
            let there = !self.gone.contains(&asked.path.as_str());
            return Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there }));
        }
        if let WorkerRequest::RemoveWorkspace(removing) = request {
            if self.cant_remove == Some(removing.path.as_str()) {
                return Err(ApiError::new(ErrorCode::Worker, "not a git repository"));
            }
            if self.refuses_to_remove == Some(removing.path.as_str()) && !removing.force {
                return Err(ApiError::new(ErrorCode::NeedsForce, "1 uncommitted change(s) would be lost"));
            }
            seen.lock().removed.push(removing.path);
        } else if let WorkerRequest::WatchChildBranches(watching) = request {
            seen.lock().watched.push(watching.path);
        }
        Ok(WorkerResponse::Ok)
    }

    /// What git says of the worktree at `path`.
    fn status(self, path: &str) -> Result<GitStatus, ApiError> {
        if self.gone.contains(&path) {
            return Err(ApiError::new(ErrorCode::Worker, "not a git repository"));
        }
        let own = path == "/ws/alder";
        let uncommitted = GitFile { path: "a.txt".into(), orig_path: None, status: ".M".into() };
        Ok(GitStatus {
            head: Some("abc".into()),
            unreferenced_commits: self.others_lose && !own,
            files: if self.own_loses && own { vec![uncommitted] } else { vec![] },
            ..GitStatus::default()
        })
    }
}

/// A hub whose workspace `w`, `alder`, has its worktree at `/ws/alder`, on `alder/start`, and a stand-in for its
/// worker that answers as `worker` says.
fn workspace(dir: &std::path::Path, worker: StandIn) -> (Arc<Hub>, Arc<WorkerLink>, Arc<Mutex<Seen>>) {
    let hub = hub_in(dir);
    hub.store.workspace_ready(&"w".into(), "/ws/alder", &"alder/start".into(), "main", None).unwrap();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let noted = seen.clone();
    let link = WorkerLink::answering(move |request| Some(worker.answer(&noted, request)));
    hub.workers.lock().insert("w".into(), link.clone());
    (hub, link, seen)
}

fn new_branch(name: &str) -> NewWorktreeOn {
    NewWorktreeOn::NewBranch { name: WorkName::try_from(name.to_string()).unwrap(), base: None }
}

/// What the hub answers the worker of `w` asking `ask` for the session `s`, read as the ask's own answer.
async fn ask<A: Ask>(hub: &Arc<Hub>, link: &WorkerLink, ask: A) -> Result<A::Answer, ApiError> {
    let reply = hub.answer_ask(&"w".into(), link, "s".into(), ask.into_ask()).await?;
    Ok(A::Answer::read(reply).unwrap_or_else(|other| panic!("another kind of answer: {other:?}")))
}

async fn create(hub: &Arc<Hub>, link: &WorkerLink, on: NewWorktreeOn, key: &str) -> Result<WorktreeCreated, ApiError> {
    ask(hub, link, CreateWorktree { on, key: AskKey::from(key) }).await
}

async fn remove(hub: &Arc<Hub>, link: &WorkerLink, path: &str, force: bool) -> Result<WorktreeRemoval, ApiError> {
    ask(hub, link, RemoveWorktree { path: path.to_string(), force }).await
}

/// Records the worktree `work` as being made for the ask `key`, as a hub that stopped before its worker answered
/// leaves it.
fn left_being_made(hub: &Hub, work: &str, key: &str) -> ChildWorktree {
    let child = ChildWorktree {
        path: at(work),
        branch: BranchName::from(format!("alder/{work}")),
        made_by: MadeBy::Undetermined,
        status: ChildStatus::Creating,
        ..fixtures::child_worktree(work)
    };
    hub.store.insert_child(&child, &key.into()).unwrap();
    child
}

/// Waits until the stand-in has taken everything sent to it so far, such as a watch nobody waits for, since it
/// answers in order.
async fn taken_so_far(link: &WorkerLink) {
    link.call(worker::GitStatus { path: "/ws/alder".into() }).await.unwrap();
}

/// Where the workspace's other worktree `try` goes.
const TRY: &str = "/ws/.children/alder/try";

/// Something that mustn't happen has no condition to wait on, so this is a bound. Right code never gets past it.
const BOUND: Duration = Duration::from_millis(300);

/// A new worktree's branch goes in the workspace's folder, a name the worker says is taken gets the next number, and
/// the one made is recorded and watched.
#[tokio::test]
async fn a_new_worktree_goes_in_the_workspace_s_folder_and_a_taken_name_gets_a_number() {
    let dir = tempfile::tempdir().unwrap();
    let (taken, next) = (at("try"), at("try-2"));
    let (hub, link, seen) = workspace(dir.path(), StandIn { taken: &[TRY], ..StandIn::default() });

    let WorktreeCreated { worktree, commit, copied, .. } = create(&hub, &link, new_branch("try"), "k").await.unwrap();

    assert_eq!((worktree.branch.as_str(), worktree.path.as_str()), ("alder/try-2", next.as_str()));
    assert_eq!((&worktree.status, commit.as_str(), copied), (&ChildStatus::Ready, "abc", vec![".env".to_string()]));
    let asked: Vec<_> = seen.lock().added.iter().map(|(path, _)| path.clone()).collect();
    assert_eq!(asked, [taken, next.clone()]);
    let new = ChildBranch::New { branch: BranchName::from("alder/try-2"), base: "HEAD".into() };
    assert_eq!(seen.lock().added[1].1, new);
    let kept = hub.store.every_child_of(&"w".into()).unwrap();
    assert_eq!(kept, std::slice::from_ref(&worktree), "the refused one is forgotten");
    taken_so_far(&link).await;
    assert_eq!(seen.lock().watched, [next]);
}

/// A worker asks each new hub what the last didn't answer, so the same ask answers with what it made.
#[tokio::test]
async fn asking_again_with_the_same_key_answers_with_what_it_made_once() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());

    let first = create(&hub, &link, new_branch("try"), "k").await.unwrap();
    let again = create(&hub, &link, new_branch("try"), "k").await.unwrap();

    assert_eq!(first.worktree.id, again.worktree.id);
    assert_eq!(seen.lock().added.len(), 1);
}

/// A worktree on a branch that exists has to be on one of the workspace's, which the hub tells from its records, such
/// as one another of its worktrees has had.
#[tokio::test]
async fn a_worktree_on_a_branch_that_exists_is_only_on_the_workspace_s() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());
    let on = |branch: &str| NewWorktreeOn::Existing { branch: BranchName::from(branch) };

    let refused = create(&hub, &link, on("main"), "k1").await.unwrap_err();
    create(&hub, &link, on("alder/x/y"), "k2").await.unwrap();
    let fix = Branch { name: BranchName::from("fix"), tip: "abc".into(), upstream: None };
    let switched = Branches { head: Head::Branch { name: fix.name.clone() }, had: vec![fix] };
    hub.store.set_child_branches(&"w".into(), &at("x-y"), &switched).unwrap();
    let in_another = create(&hub, &link, on("fix"), "k3").await;

    assert_eq!(refused.code, ErrorCode::NotFound);
    let existing = |branch: &str| ChildBranch::Existing { branch: BranchName::from(branch) };
    assert!(in_another.is_ok(), "one of its other worktrees has had it: {in_another:?}");
    assert_eq!(seen.lock().added, [(at("x-y"), existing("alder/x/y")), (at("fix"), existing("fix"))]);
}

/// The list has the workspace's own worktree and each other one that's there or being made, with what its git says,
/// which is nothing for one that has gone without the hub knowing.
#[tokio::test]
async fn the_list_has_the_workspace_s_own_and_each_other_that_s_there_or_being_made() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn { gone: &["/ws/.children/alder/lost"], ..StandIn::default() });
    for (name, key) in [("removed", "k1"), ("here", "k2"), ("lost", "k3")] {
        create(&hub, &link, new_branch(name), key).await.unwrap();
    }
    remove(&hub, &link, &at("removed"), false).await.unwrap();
    left_being_made(&hub, "making", "k4");

    let Worktrees { own, children } = ask(&hub, &link, ListWorktrees).await.unwrap();

    assert_eq!((own.path.as_str(), own.status.and_then(|status| status.head)), ("/ws/alder", Some("abc".into())));
    let listed: Vec<_> = children
        .iter()
        .map(|child| (child.worktree.path.clone(), child.status.as_ref().and_then(|status| status.head.clone())))
        .collect();
    // Oldest first, and the one left being made is the fixture's, from before the others.
    let making = (at("making"), Some("abc".into()));
    assert_eq!(listed, [making, (at("here"), Some("abc".into())), (at("lost"), None)]);
}

/// A worker asks each new hub what the last didn't answer. One the last was making is asked of the worker again, where
/// it was to be, which the worker answers with what it made, rather than made again under the first name free.
#[tokio::test]
async fn one_a_hub_stopped_while_making_is_finished_where_it_was_to_be() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, seen) = workspace(dir.path(), StandIn::default());
    // At the second name, as when the first was taken, so finishing it there isn't trying the first again.
    let earlier = left_being_made(&hub, "try-2", "k");

    let WorktreeCreated { worktree, .. } = create(&hub, &link, new_branch("try"), "k").await.unwrap();

    assert_eq!((worktree.id, worktree.status), (earlier.id, ChildStatus::Ready));
    let new = ChildBranch::New { branch: earlier.branch, base: "HEAD".into() };
    assert_eq!(seen.lock().added, [(at("try-2"), new)]);
}

/// One being made whose name the worker says was taken is forgotten, and the ask makes the next name.
#[tokio::test]
async fn one_being_made_whose_name_was_taken_is_forgotten_for_the_next_name() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn { taken: &[TRY], ..StandIn::default() });
    left_being_made(&hub, "try", "k");

    let WorktreeCreated { worktree, .. } = create(&hub, &link, new_branch("try"), "k").await.unwrap();

    assert_eq!(worktree.branch.as_str(), "alder/try-2");
    assert_eq!(hub.store.every_child_of(&"w".into()).unwrap(), [worktree], "the one being made is forgotten");
}

/// A worker may make one whose link went before it answered, so its record stays, for asking again to finish. One the
/// worker answers with an error is forgotten.
#[tokio::test]
async fn one_whose_worker_went_before_answering_stays_being_made() {
    let dir = tempfile::tempdir().unwrap();
    let cant_make = StandIn { cant_make: Some("/ws/.children/alder/broken"), ..StandIn::default() };
    let (hub, link, _) = workspace(dir.path(), cant_make);

    let lost = create(&hub, &WorkerLink::unconnected(), new_branch("try"), "k").await.unwrap_err();
    let kept = hub.store.every_child_of(&"w".into()).unwrap();
    let again = create(&hub, &link, new_branch("try"), "k").await.unwrap();
    let failed = create(&hub, &link, new_branch("broken"), "k2").await.unwrap_err();

    assert_eq!(lost.code, ErrorCode::Worker);
    assert_eq!(kept.iter().map(|child| &child.status).collect::<Vec<_>>(), [&ChildStatus::Creating]);
    assert_eq!(again.worktree.id, kept[0].id, "finished");
    assert_eq!(failed.code, ErrorCode::Conflict);
    assert_eq!(hub.store.every_child_of(&"w".into()).unwrap(), [again.worktree], "the one refused is forgotten");
}

/// One the worker made that the hub then couldn't record as there stays recorded as being made, rather than forgotten
/// with its worktree left behind, so asking again finishes it.
#[tokio::test]
async fn one_made_that_the_hub_couldnt_record_stays_being_made() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, link, _) = workspace(dir.path(), StandIn::default());
    // Another connection to the hub's database, which only breaks recording one as there.
    let database = rusqlite::Connection::open(dir.path().join("hub.db")).unwrap();
    let refuse = "SELECT RAISE(ABORT, 'broken')";
    database
        .execute_batch(&format!("CREATE TRIGGER broken BEFORE UPDATE OF state ON child_worktrees BEGIN {refuse}; END"))
        .unwrap();

    let failed = create(&hub, &link, new_branch("try"), "k").await.unwrap_err();
    let kept = hub.store.every_child_of(&"w".into()).unwrap();
    database.execute_batch("DROP TRIGGER broken").unwrap();
    let again = create(&hub, &link, new_branch("try"), "k").await.unwrap();

    assert_eq!(failed.code, ErrorCode::Internal);
    assert_eq!(kept.iter().map(|child| &child.status).collect::<Vec<_>>(), [&ChildStatus::Creating]);
    assert_eq!(again.worktree.id, kept[0].id, "finished");
}
