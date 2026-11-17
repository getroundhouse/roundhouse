use super::*;

/// As `git worktree remove` leaves one it couldn't wholly delete, such as for a file held open: its record gone, and
/// what's left of its folder, `.git` file and all.
#[tokio::test]
async fn a_worktree_git_has_forgotten_is_deleted_only_when_forced() {
    let archived = Archived::ready().await;
    std::fs::remove_dir_all(archived.record()).unwrap();

    let refused = archived.remove(false).await.err().unwrap();

    assert_eq!(
        refused.code,
        ErrorCode::NeedsForce,
        "so whoever asked is offered a forced archive: {}",
        refused.message
    );
    assert!(archived.path.join("ignored/deep/output").exists(), "a refused archive must not touch files");
    archived.remove(true).await.unwrap();
    assert!(!archived.path.exists());
    assert!(archived.has_branch());
    assert_eq!(archived.worktrees(), 1);
    archived.restore().await.unwrap();
    assert!(archived.path.join("a.txt").exists());
    archived.wait_until_deleted().await;
}

/// Only a folder that names a record of this repository's is taken for one of its worktrees.
#[tokio::test]
async fn a_folder_naming_another_repository_s_record_is_left_alone() {
    let archived = Archived::ready().await;
    let elsewhere = archived.dir.path().join("elsewhere");
    run(archived.dir.path(), &["init", "-q", elsewhere.to_str().unwrap()]);
    std::fs::remove_dir_all(archived.record()).unwrap();
    let named = elsewhere.join(".git/worktrees/alder");
    std::fs::write(archived.path.join(".git"), format!("gitdir: {}\n", named.display())).unwrap();

    let forced = archived.remove(true).await.err().unwrap();

    assert_eq!(forced.code, ErrorCode::Worker, "{}", forced.message);
    assert!(archived.path.join("ignored/deep/output").exists());
    assert!(archived.waiting().is_empty());
}
