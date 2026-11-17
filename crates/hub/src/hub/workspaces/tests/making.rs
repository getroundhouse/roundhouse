//! Making a workspace, with a stand-in for its worker, and settling one an earlier hub left part way through being
//! archived or restored.

use protocol::client::{ApiError, ErrorCode, HubNotice};
use protocol::model::{AgentKind, ClientRequestId, RecordKey, SessionSettingsChange, WorkspaceId, WorkspaceStatus};
use protocol::worker::{PrepareWorkspace, RepoSource, WorkerRequest, WorkerResponse, WorktreeThereReply};

use crate::hub::stand_in_workers::{hub_whose_stand_ins_answer_first, hub_with_stand_in_workers};
use crate::hub::tests::{hub_in, serving};
use crate::store::{Ended, Move};
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
    let (hub, _) = hub_whose_stand_ins_answer_first(dir.path(), |request| {
        let WorkerRequest::PrepareWorkspace(_) = request else { return None };
        Some(Err(ApiError::new(ErrorCode::Worker, "the repository has gone")))
    });
    let mut feed = hub.subscribe_feed();

    let refused = hub.workspace_create("r".into(), Some("doomed".into()), None, None).await.map_err(|e| e.code);

    let told: Vec<_> = std::iter::from_fn(|| feed.try_recv().ok())
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
        let (unchanged, request) = (SessionSettingsChange::default(), ClientRequestId::from(request));
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
#[tokio::test]
async fn a_move_cut_short_is_settled_by_whether_its_worker_says_the_worktree_is_there() {
    use WorkspaceStatus::{Archived, Ready};
    use protocol::model::{Branch, Branches, Head, SetupStatus};
    use protocol::worker::SetupDone;
    let there = |there| Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there }));
    let cant_say = Err(ApiError::new(ErrorCode::Worker, "a worker that doesn't know the question"));
    let (seen_on, made_on) = ("fix-the-login-bug", "alder");
    let last_seen = Branch { name: seen_on.into(), tip: "c1".into(), upstream: None };
    let last_seen = Branches { head: Head::Branch { name: seen_on.into() }, had: vec![last_seen] };
    let cases = [
        ("an archive that left the worktree", Move::Archive, there(true), (Ready, made_on, false)),
        ("an archive that removed it", Move::Archive, there(false), (Archived, seen_on, false)),
        ("a restore that put it back", Move::Restore, there(true), (Ready, made_on, true)),
        ("a restore that didn't", Move::Restore, there(false), (Archived, made_on, false)),
        ("an archive the worker can't say of", Move::Archive, cant_say, (Ready, made_on, false)),
    ];

    let mut wrong = Vec::new();
    for (name, moving, said, settled) in cases {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub_in(dir.path());
        let id = WorkspaceId::from("w");
        let worktree = dir.path().join("ws/alder");
        std::fs::create_dir_all(&worktree).unwrap();
        hub.store.workspace_ready(&id, worktree.to_str().unwrap(), &made_on.into(), "main", None).unwrap();
        hub.store.set_workspace_branches(&id, &last_seen).unwrap();
        hub.store.start_move(&id, Move::Archive).unwrap();
        if moving == Move::Restore {
            hub.store.end_move(&id, Move::Archive, Ended::Done).unwrap();
            hub.store.start_move(&id, Move::Restore).unwrap();
        }
        // Its setup script passes, and it does whatever else it's asked.
        let link = WorkerLink::answering(move |request| {
            Some(if let WorkerRequest::WorktreeThere(_) = request {
                said.clone()
            } else if let WorkerRequest::SetupWorkspace(_) = request {
                Ok(WorkerResponse::SetupDone(SetupDone { code: Some(0) }))
            } else {
                Ok(WorkerResponse::Ok)
            })
        });
        serving(&hub, &link);

        hub.clone().settle_moves(vec![(id.clone(), moving)]).await;

        let workspace = hub.store.get_workspace(&id).unwrap();
        let set_up_again = workspace.setup != SetupStatus::NotStarted;
        let got = (workspace.status, workspace.branch.as_str(), set_up_again);
        if got != settled {
            wrong.push(format!("{name}: {got:?}, not {settled:?}"));
        }
    }
    assert!(wrong.is_empty(), "status, branch and whether it was set up again, as each was settled: {wrong:#?}");
}
