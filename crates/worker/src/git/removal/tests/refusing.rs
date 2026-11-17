use super::*;
use crate::testing::given;

#[tokio::test]
async fn a_worktree_holding_another_repository_is_refused_unless_forced() {
    let archived = Archived::ready().await;
    let nested = archived.path.join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    run(&nested, &["init", "-q", "-b", "main"]);
    std::fs::write(nested.join("only-here.txt"), "work\n").unwrap();
    run(&nested, &["add", "."]);
    run(&nested, &["commit", "-qm", "a commit nowhere else"]);
    run(&archived.path, &["add", "nested"]);
    run(&archived.path, &["commit", "-qm", "the repository inside"]);
    assert_eq!(status(&given(), archived.path.to_str().unwrap()).await.unwrap().loss_if_removed(), None);

    let refused = archived.remove(false).await.err().unwrap();

    assert_eq!(
        refused.code,
        ErrorCode::NeedsForce,
        "so whoever asked is offered a forced archive: {}",
        refused.message
    );
    assert!(nested.join("only-here.txt").exists());
    archived.remove(true).await.unwrap();
    assert!(!archived.path.exists());
    archived.wait_until_deleted().await;
}

/// As git decides, a gitlink with no repository checked out at its path has no commits in the worktree to lose.
#[tokio::test]
async fn a_gitlink_with_nothing_checked_out_is_archived() {
    let archived = Archived::ready().await;
    // The empty folder a submodule that isn't checked out has, without which git says it was deleted.
    std::fs::create_dir(archived.path.join("nested")).unwrap();
    let commit = run(&archived.path, &["rev-parse", "HEAD"]);
    run(&archived.path, &["update-index", "--add", "--cacheinfo", &format!("160000,{commit},nested")]);
    run(&archived.path, &["commit", "-qm", "a submodule not checked out"]);

    archived.remove(false).await.unwrap();

    assert!(!archived.path.exists());
    archived.wait_until_deleted().await;
}

/// Deleting it where it is instead could stop part way, after git had forgotten it, leaving half a worktree.
#[tokio::test]
async fn a_worktree_that_cant_be_moved_aside_is_refused_and_left_whole() {
    let archived = Archived::ready().await;
    let record = archived.record();
    std::fs::write(archived.deleting(), "in the way\n").unwrap();

    let refused = archived.remove(false).await.err().unwrap();
    let forced = archived.remove(true).await.err().unwrap();

    assert_eq!((refused.code, forced.code), (ErrorCode::Worker, ErrorCode::Worker), "{}", refused.message);
    assert!(archived.path.join("a.txt").exists());
    assert!(archived.path.join("ignored/deep/output").exists());
    assert!(record.is_dir(), "git still has it");
    assert_eq!(archived.worktrees(), 2);
}
