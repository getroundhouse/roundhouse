//! Making a workspace, with a stand-in for its worker, and settling one an earlier hub left part way through being
//! archived or restored.

use std::sync::Arc;

use parking_lot::Mutex;
use protocol::capability::settings::Settings;
use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{
    AgentKind, Branch, Branches, ClientRequestId, Head, ObservationKey, RecordKey, SetupStatus, WorkspaceId,
    WorkspaceStatus,
};
use protocol::worker::{PrepareWorkspace, RepoSource, SetupDone, WorkerRequest, WorkerResponse, WorktreeThereReply};

use crate::hub::stand_in_workers::{TIMEOUT, hub_whose_stand_ins_answer_first, hub_with_stand_in_workers};
use crate::hub::tests::{described, hub_looking_often, serving};
use crate::hub::{WorkspaceBranches, WorkspaceWorkerBuild};
use crate::store::{Ended, Move};
use crate::timers::Timers;
use crate::worker_link::WorkerLink;

/// The worktree a worker was asked to make.
fn worktree_asked_for(request: &WorkerRequest) -> Option<PrepareWorkspace> {
    let WorkerRequest::PrepareWorkspace(prepare) = request else { return None };
    Some(prepare.clone())
}

/// Its worker is asked for a worktree from the repository's checkout, on a branch in a folder of the workspace's name,
/// and where the worker says it made it is where the workspace is.
#[tokio::test]
async fn a_workspace_is_ready_where_its_worker_made_the_worktree_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());

    let made = hub.workspace_create("r".into(), Some("birch".into()), Some("origin/next".into()), None).await.unwrap();

    let asked_for = PrepareWorkspace {
        repo_name: "repo".to_string(),
        source: RepoSource::Checkout { path: "/repo".to_string() },
        workspace_name: "birch".to_string(),
        branch: "birch/start".into(),
        base_ref: Some("origin/next".to_string()),
    };
    assert_eq!(asked.requests(worktree_asked_for), [asked_for]);
    assert_eq!(
        (made.status, made.path.as_deref(), made.branch.as_str(), made.base_ref.as_deref()),
        (WorkspaceStatus::Ready, Some("/ws/birch"), "birch/start", Some("origin/next"))
    );
    assert_eq!(hub.workspace(&made.id).await.unwrap().path.as_deref(), Some("/ws/birch"), "as its record says");
}

/// So a client shows it on its way until it goes: it's announced as soon as it's recorded, and once its worker couldn't
/// make the worktree, its removal, which is newer, and nothing of it is left.
#[tokio::test]
async fn a_workspace_that_cant_be_made_is_announced_on_its_way_and_then_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_whose_stand_ins_answer_first(dir.path(), vec![described()], |request| {
        let WorkerRequest::PrepareWorkspace(_) = request else { return None };
        Some(Err(ApiError::new(ErrorCode::Worker, "the repository has gone")))
    });
    let mut feed = hub.subscribe_feed();

    let refused = hub.workspace_create("r".into(), Some("doomed".into()), None, None).await.map_err(|e| e.code);

    let told: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok().map(std::sync::Arc::unwrap_or_clone))
        .filter(|notice| matches!(notice, HubNotice::Workspace(_) | HubNotice::Removed { .. }))
        .collect();
    let [HubNotice::Workspace(making), HubNotice::Removed { record, rev }] = told.as_slice() else {
        panic!("the workspace on its way, then its removal: {told:?}")
    };
    assert_eq!(refused.err(), Some(ErrorCode::Worker));
    assert_eq!((making.name.as_str(), making.status), ("doomed", WorkspaceStatus::Creating));
    assert_eq!(record, &RecordKey::Workspace { id: making.id.clone() });
    assert!(*rev > making.rev, "{rev:?} after {:?}", making.rev);
    assert!(hub.workspace_list(true).await.unwrap().iter().all(|workspace| workspace.id != making.id));
}

/// So a client that loses the reply can send it again, whatever has happened meanwhile, and get what it made rather
/// than a refusal, or a second one. Here the workspace has been archived since, where a new chat is refused.
#[tokio::test]
async fn a_request_sent_again_gets_what_it_made_however_things_have_changed_since() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_with_stand_in_workers(dir.path());
    let a_workspace = || hub.workspace_create("r".into(), None, None, Some("a workspace".into()));
    let workspace = a_workspace().await.unwrap();
    let a_chat = |request: &str| {
        let (unchanged, request) = (Settings::default(), ClientRequestId::from(request));
        hub.session_create(workspace.id.clone(), AgentKind::Claude, unchanged, Some(request))
    };
    let chat = a_chat("a chat").await.unwrap();
    hub.workspace_archive(workspace.id.clone(), true).await.unwrap();

    let (workspace_again, chat_again) = (a_workspace().await.unwrap(), a_chat("a chat").await.unwrap());

    assert_eq!((&workspace_again.id, workspace_again.status), (&workspace.id, WorkspaceStatus::Archived));
    assert_eq!(chat_again.id, chat.id);
    assert_eq!(asked.requests(worktree_asked_for).len(), 1, "its worker wasn't asked for a second worktree");
    let another = a_chat("another chat").await.map_err(|refused| refused.code);
    assert_eq!(another.err(), Some(ErrorCode::Conflict), "a new one is checked, and refused");
}

/// How far an earlier hub got with archiving or restoring a workspace is for the worker that was doing it to say, by
/// whether the worktree is there once it has finished with it. The hub records what it says, even with the worktree's
/// folder there, which git makes before it has finished with it. An archive that left the worktree, or a restore that
/// didn't put it back, got no further than that, so the workspace is as it was. An archive that removed it is done, on
/// the branch the worktree was last seen on, and a restore that put it back is done, and sets the worktree up again,
/// as one that wasn't cut short does. One the worker can't say of is put back as it was.
///
/// Then its worker is dealt with as the workspace now is: an archived one's is asked to stop, and a ready one's to
/// watch the worktree's branches, as one the hub carries on with is, and to stop once nothing is at work in it, if
/// it's another build's, which build it is said meanwhile. Each is asked once: a restore that put the worktree back has it watch them as it's recorded,
/// as one that wasn't cut short does, and not again once it's settled.
#[tokio::test]
async fn a_move_cut_short_is_settled_by_whether_its_worker_says_the_worktree_is_there() {
    use WorkspaceStatus::{Archived, Ready};
    let there = |there| Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there }));
    let cant_say = Err(ApiError::new(ErrorCode::Worker, "a worker that's gone"));
    let (seen_on, made_on, ours, another) = ("fix-the-login-bug", "alder", None, Some("0.9.0"));
    let last_seen = Branch { name: seen_on.into(), tip: "c1".into(), upstream: None };
    let last_seen = Branches { head: Head::Branch { name: seen_on.into() }, had: vec![last_seen] };
    let (watched, stopped, replaced) = (&[WATCH][..], &[STOP][..], &[WATCH, STOP][..]);
    let cases = [
        ("an archive that left the worktree", Move::Archive, there(true), ours, (Ready, made_on, false), watched),
        ("an archive that removed it", Move::Archive, there(false), ours, (Archived, seen_on, false), stopped),
        ("a restore that put it back", Move::Restore, there(true), ours, (Ready, made_on, true), watched),
        ("a restore that didn't", Move::Restore, there(false), ours, (Archived, made_on, false), stopped),
        ("an archive the worker can't say of", Move::Archive, cant_say, ours, (Ready, made_on, false), watched),
        ("an archive another build left", Move::Archive, there(true), another, (Ready, made_on, false), replaced),
    ];

    let (mut wrong, mut settled_by) = (Vec::new(), Vec::new());
    for (name, moving, said, build, settled, dealt_with) in cases {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_looking_often(dir.path());
        let id = WorkspaceId::from("w");
        let worktree = dir.path().join("ws/alder");
        std::fs::create_dir_all(&worktree).unwrap();
        hub.store.workspace_ready(&id, worktree.to_str().unwrap(), &made_on.into(), "main").unwrap();
        hub.branches_known(&id, last_seen.clone()).await;
        hub.store.start_move(&id, Move::Archive).unwrap();
        if moving == Move::Restore {
            hub.store.end_move(&id, Move::Archive, Ended::Done).unwrap();
            hub.store.start_move(&id, Move::Restore).unwrap();
        }
        let (link, asked) = worker_saying(said);
        serving(&hub, &build.map_or(link.clone(), |build| link.of_build(build)));

        hub.clone().settle_moves(vec![(id.clone(), moving)]).await;

        let workspace = hub.store.get_workspace(&id).unwrap();
        let set_up_again = workspace.setup != SetupStatus::NotStarted;
        let got = (workspace.status, workspace.branch.as_str(), set_up_again);
        if got != settled {
            wrong.push(format!("{name}: {got:?}, not {settled:?}"));
        }
        if hub.observations.keeps::<WorkspaceWorkerBuild>(&id) != build.is_some() {
            wrong.push(format!("{name}: which build its worker is wasn't said while it's kept to be replaced"));
        }
        if waiting::task::up_to(TIMEOUT, || *asked.lock() == dealt_with).await {
            settled_by.push((name, dir, hub, asked, dealt_with));
        } else {
            wrong.push(format!("{name}: its worker was asked {:?}, not {dealt_with:?}", asked.lock()));
        }
    }
    // Nothing says a worker won't be replaced, so each hub is given a few looks to replace one it mustn't.
    waiting::task::for_what_mustnt_happen(Timers::FOR_TESTS.look_every * 5).await;
    for (name, _, _, asked, dealt_with) in &settled_by {
        if *asked.lock() != *dealt_with {
            wrong.push(format!("{name}: its worker was then asked {:?}, not only {dealt_with:?}", asked.lock()));
        }
    }
    assert!(wrong.is_empty(), "status, branch, whether it was set up again, and what its worker was asked: {wrong:#?}");
}

/// A restore put back leaves the workspace archived, so what was said of its worktree while it was being restored,
/// which was kept, since the worktree was on its way back, is forgotten, and clients are told.
#[tokio::test]
async fn a_restore_put_back_forgets_what_was_said_of_the_worktree_meanwhile() {
    let dir = tempfile::tempdir().unwrap();
    let hub = hub_looking_often(dir.path());
    let id = WorkspaceId::from("w");
    hub.store.workspace_ready(&id, "/ws/alder", &"alder".into(), "main").unwrap();
    hub.store.start_move(&id, Move::Archive).unwrap();
    hub.store.end_move(&id, Move::Archive, Ended::Done).unwrap();
    hub.store.start_move(&id, Move::Restore).unwrap();
    let branches = Branches { head: Head::Detached { commit: "c1".into() }, had: Vec::new() };
    hub.branches_known(&id, branches).await;
    let kept_while_restoring = hub.observations.keeps::<WorkspaceBranches>(&id);
    let (link, _) = worker_saying(Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there: false })));
    serving(&hub, &link);
    let mut feed = hub.subscribe_feed();

    hub.clone().settle_moves(vec![(id.clone(), Move::Restore)]).await;

    assert!(kept_while_restoring, "said while it was being restored");
    assert_eq!(hub.store.get_workspace(&id).unwrap().status, WorkspaceStatus::Archived, "put back");
    assert!(!hub.observations.keeps::<WorkspaceBranches>(&id), "forgotten as it was put back");
    let removed = std::iter::from_fn(|| feed.try_recv().ok()).any(|notice| {
        matches!(&*notice, HubNotice::ObservationRemoved { observation: ObservationKey::WorkspaceBranches { .. }, .. })
    });
    assert!(removed, "clients told");
}

/// One of what a worker is asked that says what becomes of it: to watch the worktree's branches.
const WATCH: &str = "watch the branches";
/// The other: to stop.
const STOP: &str = "stop";

/// A stand-in for a worker that says `said` of whether the worktree is there, passes its setup script, and does
/// whatever else it's asked, and which of [`WATCH`] and [`STOP`] it was asked, in order.
fn worker_saying(said: Result<WorkerResponse, ApiError>) -> (Arc<WorkerLink>, Arc<Mutex<Vec<&'static str>>>) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let kept = asked.clone();
    let link = WorkerLink::answering(move |request| {
        if let WorkerRequest::WatchBranches(_) = request {
            kept.lock().push(WATCH);
        }
        if let WorkerRequest::Stop = request {
            kept.lock().push(STOP);
        }
        Some(if let WorkerRequest::WorktreeThere(_) = request {
            said.clone()
        } else if let WorkerRequest::SetupWorkspace(_) = request {
            Ok(WorkerResponse::SetupDone(SetupDone { code: Some(0) }))
        } else {
            Ok(WorkerResponse::Ok)
        })
    });
    (link, asked)
}
