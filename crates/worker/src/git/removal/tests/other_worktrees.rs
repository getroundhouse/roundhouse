use super::*;

/// Another of a workspace's worktrees waits to be deleted where the workspace's own would, beside the workspaces,
/// where every archive clears what waits, and the last of them to go takes its workspace's folder with it.
#[tokio::test]
async fn another_of_a_workspace_s_worktrees_is_moved_aside_beside_the_workspaces() {
    let archived = Archived::ready().await;
    let folder = archived.path.with_file_name(".children").join("alder");
    let (first, last) = (folder.join("a"), folder.join("b"));
    for (child, branch) in [(&first, "alder-a"), (&last, "alder-b")] {
        run(&archived.path, &["worktree", "add", "-q", "-b", branch, child.to_str().unwrap()]);
    }
    let remove = |child: &Path| archived.removing(child, false, Some(&archived.path));

    remove_workspace(&remove(&first), &deleter()).await.unwrap();
    let left_after_the_first = (first.exists(), folder.exists());
    remove_workspace(&remove(&last), &deleter()).await.unwrap();

    assert_eq!(left_after_the_first, (false, true), "the folder stays while another is in it");
    assert!(archived.deleting().join(LOCK).exists(), "moved aside where the workspace's own would be");
    assert!(!folder.exists(), "nothing of the workspace's is left in `.children/`, nor a place to delete from");
    assert!(archived.path.exists(), "the workspace's own stays");
}
