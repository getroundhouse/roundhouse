use super::Archived;
use crate::git::worktrees::tests::while_one_is_made;

/// Having git forget the worktree changes the repository's worktrees.
#[tokio::test]
async fn archiving_waits_for_a_worktree_of_the_repository_being_made() {
    let archived = Archived::ready().await;

    while_one_is_made(&archived.repo, archived.remove(false)).await.unwrap();

    assert!(!archived.path.exists());
    assert_eq!(archived.worktrees(), 1, "git still has it");
    archived.wait_until_deleted().await;
}

/// One deleted by hand is forgotten with a `git worktree prune`, which changes the repository's worktrees too.
#[tokio::test]
async fn archiving_one_deleted_by_hand_waits_for_a_worktree_of_the_repository_being_made() {
    let archived = Archived::ready().await;
    std::fs::remove_dir_all(&archived.path).unwrap();

    while_one_is_made(&archived.repo, archived.remove(false)).await.unwrap();

    assert_eq!(archived.worktrees(), 1, "git still has it");
}
