//! Archiving, restoring, bundle persistence, and crash settling on `ClonePersistence::Ephemeral` compute.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use protocol::client::{ApiError, ErrorCode};
use protocol::model::{GitFile, GitStatus, RepoUrl, WorkspaceStatus};
use protocol::provider::{ClonePersistence, WorkerEnvironment, WorkerFilesystem};
use protocol::worker::{
    ArchivedBranch, ArchivedBundle, ArchivedGit, CaptureArchive, RestoreWorkspace, WorkerRequest, WorkerResponse,
    WorktreeThereReply,
};

use crate::hub::Hub;
use crate::hub::stand_in_workers::{Asked, hub_with_environment};
use crate::hub::tests::described;
use crate::store::{InstanceStage, Move};

fn ephemeral_env() -> WorkerEnvironment {
    WorkerEnvironment { filesystem: WorkerFilesystem::Separate, clones: ClonePersistence::Ephemeral }
}

fn remote_url() -> RepoUrl {
    RepoUrl::try_from("https://example.invalid/org/repo.git".to_owned()).unwrap()
}

fn sample_archived() -> ArchivedGit {
    ArchivedGit {
        branches: vec![ArchivedBranch { name: "birch/start".into(), commit: "c0ffee".into() }],
        bundle: Some(ArchivedBundle::from(b"BODY".as_slice())),
        bundle_omitted: false,
    }
}

fn hub_ephemeral(
    dir: &std::path::Path,
    first: impl Fn(&WorkerRequest) -> Option<Result<WorkerResponse, ApiError>> + Send + Sync + 'static,
) -> (Arc<Hub>, Arc<Asked>) {
    let (hub, asked) = hub_with_environment(dir, ephemeral_env(), vec![described()], first);
    hub.store.read_repo_url(&"r".into(), "/repo", Some(&remote_url()), None).unwrap();
    (hub, asked)
}

#[tokio::test]
async fn ephemeral_archive_persists_bundle_before_destroy_and_restore_sends_exact_payload() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_ephemeral(dir.path(), |_| None);

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    let meta_file = dir.path().join("bundles").join(format!("{}.json", made.id));
    let bundle_file = dir.path().join("bundles").join(format!("{}.bundle", made.id));
    let (meta_seen, bundle_seen) = (meta_file.clone(), bundle_file.clone());
    asked.on_destroy(move |_| {
        assert!(meta_seen.exists(), "metadata commit marker must exist before destroy starts");
        assert!(bundle_seen.exists(), "raw bundle must exist before destroy starts");
    });

    let archived = hub.workspace_archive(made.id.clone(), false).await.unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    assert_eq!(asked.instances_destroyed(), 1);
    assert!(hub.store.recorded_instance(&made.instance_id).unwrap().is_none());
    assert_eq!(std::fs::read(&bundle_file).unwrap(), b"BODY");
    let captures = asked.requests(|r| {
        let WorkerRequest::CaptureArchive(CaptureArchive { force, .. }) = r else { return None };
        Some(*force)
    });
    assert_eq!(captures, [false]);

    let restored = hub.workspace_restore(made.id.clone()).await.unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(asked.instances_created(), 2);
    assert!(!meta_file.exists(), "metadata removed once Ready commits");
    assert!(!bundle_file.exists(), "raw bundle removed once Ready commits");
    let sent_bundles = asked.requests(|r| {
        let WorkerRequest::RestoreWorkspace(RestoreWorkspace { archived, .. }) = r else { return None };
        Some(archived.clone())
    });
    assert_eq!(sent_bundles, [Some(sample_archived())]);
}

#[tokio::test]
async fn restoring_an_older_builds_archived_ephemeral_workspace_without_bundle_restores_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_ephemeral(dir.path(), |_| None);

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    // Simulate a workspace archived by an older build (PR 1): Archived in SQLite, instance still Ready, no bundle.
    hub.store.start_move(&made.id, Move::Archive).unwrap();
    hub.store.end_move(&made.id, Move::Archive, crate::store::Ended::Done).unwrap();
    assert_eq!(hub.store.recorded_instance(&made.instance_id).unwrap().unwrap().stage, InstanceStage::Ready);

    let restored = hub.workspace_restore(made.id.clone()).await.unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(asked.instances_destroyed(), 0, "must not destroy the only copy of an older build's workspace");
    assert_eq!(asked.instances_created(), 1, "reuses the existing Ready instance");
    let sent_bundles = asked.requests(|r| {
        let WorkerRequest::RestoreWorkspace(RestoreWorkspace { archived, .. }) = r else { return None };
        Some(archived.clone())
    });
    assert_eq!(sent_bundles, [None]);
}

#[tokio::test]
async fn hub_checkout_on_ephemeral_provider_never_captures_or_destroys_the_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let env = WorkerEnvironment { filesystem: WorkerFilesystem::Hub, clones: ClonePersistence::Ephemeral };
    let (hub, asked) = hub_with_environment(dir.path(), env, vec![described()], |_| None);

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    hub.workspace_archive(made.id.clone(), false).await.unwrap();
    assert_eq!(asked.instances_destroyed(), 0, "hub checkout workspaces aren't destroyed on archive");
    assert!(asked.requests(|r| matches!(r, WorkerRequest::CaptureArchive(_)).then_some(())).is_empty());
    assert!(!hub.has_archived_git(&made.id).await);
}

#[tokio::test]
async fn dirty_worktree_and_oversized_bundle_report_both_losses_together_before_removing_anything() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_ephemeral(dir.path(), |req| {
        if let WorkerRequest::GitStatus(_) = req {
            return Some(Ok(WorkerResponse::GitStatus(GitStatus {
                files: vec![GitFile { path: "dirty.txt".into(), status: "M".into(), orig_path: None }],
                ..GitStatus::default()
            })));
        }
        if let WorkerRequest::CaptureArchive(CaptureArchive { force: false, .. }) = req {
            return Some(Err(ApiError::new(
                ErrorCode::NeedsForce,
                "archived commits are 200 MiB, over the 128 MiB limit",
            )));
        }
        None
    });

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    let err = hub.workspace_archive(made.id.clone(), false).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NeedsForce);
    assert!(err.message.contains("uncommitted"), "dirty loss reported: {}", err.message);
    assert!(err.message.contains("128 MiB"), "bundle size loss reported together: {}", err.message);
    assert!(asked.requests(|r| matches!(r, WorkerRequest::RemoveWorkspace(_)).then_some(())).is_empty());
    assert_eq!(asked.instances_destroyed(), 0);
}

#[tokio::test]
async fn capture_failure_leaves_worktree_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_ephemeral(dir.path(), |req| {
        if matches!(req, WorkerRequest::CaptureArchive(_)) {
            return Some(Err(ApiError::new(ErrorCode::Unsupported, "older worker")));
        }
        None
    });

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    let err = hub.workspace_archive(made.id.clone(), false).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Unsupported);
    assert_eq!(hub.store.get_workspace(&made.id).unwrap().status, WorkspaceStatus::Ready);
    assert!(asked.requests(|r| matches!(r, WorkerRequest::RemoveWorkspace(_)).then_some(())).is_empty());
    assert_eq!(asked.instances_destroyed(), 0);
}

#[tokio::test]
async fn transient_destroy_failure_still_archives_and_restore_destroys_before_recreating() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, asked) = hub_ephemeral(dir.path(), |_| None);

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    asked.fail_destroys(true);

    let archived = hub.workspace_archive(made.id.clone(), false).await.unwrap();
    assert_eq!(archived.status, WorkspaceStatus::Archived);
    let recorded = hub.store.recorded_instance(&made.instance_id).unwrap().expect("left Destroying");
    assert_eq!(recorded.stage, InstanceStage::Destroying);
    assert!(hub.has_archived_git(&made.id).await);

    asked.fail_destroys(false);
    let restored = hub.workspace_restore(made.id.clone()).await.unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert_eq!(asked.instances_destroyed(), 2);
    assert_eq!(asked.instances_created(), 2);
    assert!(!hub.has_archived_git(&made.id).await);
}

#[tokio::test]
async fn failed_ephemeral_restore_destroys_fresh_instance_and_keeps_bundle_for_retry() {
    let dir = tempfile::tempdir().unwrap();
    let fail_restore = Arc::new(AtomicBool::new(true));
    let flag = fail_restore.clone();
    let (hub, asked) = hub_ephemeral(dir.path(), move |req| {
        if matches!(req, WorkerRequest::RestoreWorkspace(_)) && flag.load(Ordering::SeqCst) {
            return Some(Err(ApiError::new(ErrorCode::Worker, "clone failed")));
        }
        None
    });

    let made = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    hub.workspace_archive(made.id.clone(), false).await.unwrap();

    let err = hub.workspace_restore(made.id.clone()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Worker);
    assert_eq!(hub.store.get_workspace(&made.id).unwrap().status, WorkspaceStatus::Archived);
    assert_eq!(asked.instances_destroyed(), 2);
    assert!(hub.has_archived_git(&made.id).await, "bundle kept on disk for retry");

    fail_restore.store(false, Ordering::SeqCst);
    let restored = hub.workspace_restore(made.id.clone()).await.unwrap();
    assert_eq!(restored.status, WorkspaceStatus::Ready);
    assert!(!hub.has_archived_git(&made.id).await);
}

#[tokio::test]
async fn settling_interrupted_ephemeral_moves_covers_both_worktree_states_and_unfinished_instances() {
    let dir = tempfile::tempdir().unwrap();
    let there = Arc::new(AtomicBool::new(true));
    let flag = there.clone();
    let (hub, asked) = hub_ephemeral(dir.path(), move |req| {
        if matches!(req, WorkerRequest::WorktreeThere(_)) {
            return Some(Ok(WorkerResponse::WorktreeThere(WorktreeThereReply { there: flag.load(Ordering::SeqCst) })));
        }
        None
    });

    // Case 1: Archive cut short while worktree is still there -> put back to Ready, even if a stale bundle existed.
    let w1 = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    hub.save_archived_git(&w1.id, &sample_archived()).await.unwrap();
    hub.store.start_move(&w1.id, Move::Archive).unwrap();
    hub.clone().settle_moves(vec![(w1.id.clone(), Move::Archive)]).await;
    assert_eq!(hub.store.get_workspace(&w1.id).unwrap().status, WorkspaceStatus::Ready);
    assert_eq!(asked.instances_destroyed(), 0);
    assert!(!hub.has_archived_git(&w1.id).await, "stale bundle removed when worktree is still live");

    // Case 2: Archive cut short after worktree removal and bundle save -> destroys instance and records Archived.
    there.store(false, Ordering::SeqCst);
    hub.save_archived_git(&w1.id, &sample_archived()).await.unwrap();
    hub.store.start_move(&w1.id, Move::Archive).unwrap();
    hub.clone().settle_moves(vec![(w1.id.clone(), Move::Archive)]).await;
    assert_eq!(hub.store.get_workspace(&w1.id).unwrap().status, WorkspaceStatus::Archived);
    assert_eq!(asked.instances_destroyed(), 1);
    assert!(hub.has_archived_git(&w1.id).await);

    // Case 3: Restore cut short while instance is still Creating -> settles instance directly, puts back to Archived.
    hub.store.start_move(&w1.id, Move::Restore).unwrap();
    hub.store.instance_creating(&w1.instance_id, &w1.provider).unwrap();
    hub.clone().settle_moves(vec![(w1.id.clone(), Move::Restore)]).await;
    assert_eq!(hub.store.get_workspace(&w1.id).unwrap().status, WorkspaceStatus::Archived);
    assert_eq!(asked.instances_destroyed(), 2);
    assert!(hub.has_archived_git(&w1.id).await);

    // Case 4: Restore cut short after worktree was recreated -> records Ready and removes bundle.
    there.store(true, Ordering::SeqCst);
    let w2 = hub.workspace_create("r".into(), Some("cedar".into()), None, None).await.unwrap();
    hub.save_archived_git(&w2.id, &sample_archived()).await.unwrap();
    hub.store.start_move(&w2.id, Move::Archive).unwrap();
    hub.store.end_move(&w2.id, Move::Archive, crate::store::Ended::Done).unwrap();
    hub.store.start_move(&w2.id, Move::Restore).unwrap();
    hub.clone().settle_moves(vec![(w2.id.clone(), Move::Restore)]).await;
    assert_eq!(hub.store.get_workspace(&w2.id).unwrap().status, WorkspaceStatus::Ready);
    assert!(!hub.has_archived_git(&w2.id).await);
}

#[tokio::test]
async fn tidy_bundles_cleans_stale_json_and_raw_bundles_while_keeping_archived_workspaces() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, _) = hub_ephemeral(dir.path(), |_| None);

    let archived = hub.workspace_create("r".into(), Some("birch".into()), None, None).await.unwrap();
    hub.workspace_archive(archived.id.clone(), false).await.unwrap();
    let ready = hub.workspace_create("r".into(), Some("cedar".into()), None, None).await.unwrap();
    hub.save_archived_git(&ready.id, &sample_archived()).await.unwrap();
    let bundles_dir = dir.path().join("bundles");
    std::fs::write(bundles_dir.join("orphan.json"), b"{}").unwrap();
    std::fs::write(bundles_dir.join("orphan.bundle"), b"x").unwrap();
    std::fs::write(bundles_dir.join(".partial.tmp"), b"x").unwrap();

    // A file newer than tidy's cutoff (written by a concurrent archive) must be spared.
    hub.tidy_bundles_at(std::time::SystemTime::now() - std::time::Duration::from_secs(60)).await;
    assert!(bundles_dir.join("orphan.json").exists(), "files newer than tidy start are spared");

    hub.tidy_bundles_at(std::time::SystemTime::now() + std::time::Duration::from_secs(60)).await;

    assert!(bundles_dir.join(format!("{}.json", archived.id)).exists());
    assert!(bundles_dir.join(format!("{}.bundle", archived.id)).exists());
    assert!(!bundles_dir.join(format!("{}.json", ready.id)).exists());
    assert!(!bundles_dir.join(format!("{}.bundle", ready.id)).exists());
    assert!(!bundles_dir.join("orphan.json").exists());
    assert!(!bundles_dir.join("orphan.bundle").exists());
    assert!(!bundles_dir.join(".partial.tmp").exists());
}
