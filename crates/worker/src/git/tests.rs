use super::*;

fn run(cwd: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn new_repo(dir: &Path) -> PathBuf {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.join(".gitignore"), "ignored/\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "first"]);
    repo
}

async fn prepare(root: &Path, repo: &Path, name: &str, base: Option<&str>) -> Result<Prepared, ApiError> {
    prepare_workspace(root, "repo", repo.to_str().unwrap(), name, name, base).await
}

#[tokio::test]
async fn creates_a_worktree_on_a_new_branch_from_the_current_branch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    let prepared = prepare(&root, &repo, "alder", None).await.unwrap();
    assert_eq!(prepared.base_ref, "main");
    assert_eq!(run(Path::new(&prepared.path), &["branch", "--show-current"]), "alder");
    assert!(Path::new(&prepared.path).join("a.txt").exists());
}

#[tokio::test]
async fn fetches_the_latest_remote_commit_and_does_not_track_the_base() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    // origin moves on after the clone, so only a fetch can find this commit.
    std::fs::write(origin.join("b.txt"), "new\n").unwrap();
    run(&origin, &["add", "."]);
    run(&origin, &["commit", "-qm", "second"]);
    let latest = run(&origin, &["rev-parse", "HEAD"]);

    let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();
    assert_eq!(prepared.base_ref, "origin/main");
    assert!(prepared.warnings.is_empty(), "{:?}", prepared.warnings);
    let path = Path::new(&prepared.path);
    assert_eq!(run(path, &["rev-parse", "HEAD"]), latest);
    // A tracking branch would make a bare `git pull` or `git push` act on main.
    let upstream = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "@{upstream}"])
        .current_dir(path)
        .output()
        .unwrap();
    assert!(!upstream.status.success());
}

#[tokio::test]
async fn an_unreachable_remote_warns_and_uses_the_local_copy() {
    let dir = tempfile::tempdir().unwrap();
    let origin = new_repo(dir.path());
    let clone = dir.path().join("clone");
    run(dir.path(), &["clone", "-q", origin.to_str().unwrap(), clone.to_str().unwrap()]);
    std::fs::remove_dir_all(&origin).unwrap();
    let prepared = prepare(&dir.path().join("ws"), &clone, "alder", None).await.unwrap();
    assert_eq!(prepared.warnings.len(), 1, "{:?}", prepared.warnings);
    assert!(Path::new(&prepared.path).join("a.txt").exists());
}

#[tokio::test]
async fn existing_branches_paths_and_bad_input_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let root = dir.path().join("ws");
    prepare(&root, &repo, "alder", None).await.unwrap();
    // Same path and branch.
    assert_eq!(prepare(&root, &repo, "alder", None).await.err().unwrap().code, ErrorCode::Conflict);
    // Same branch, different path.
    let err = prepare_workspace(&root, "repo", repo.to_str().unwrap(), "birch", "alder", None).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::Conflict);
    let err = prepare(&root, &repo, "cedar", Some("--upload-pack=evil")).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
    let err = prepare(&root, &repo, "cedar", Some("no-such-ref")).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
    let err = prepare_workspace(&root, "repo", repo.to_str().unwrap(), "x", "--bad", None).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::InvalidRequest);
}

#[tokio::test]
async fn archiving_refuses_uncommitted_work_unless_forced_and_keeps_the_branch() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    let repo_str = repo.to_str().unwrap();

    std::fs::write(path.join("new.txt"), "work\n").unwrap();
    let err = remove_workspace(repo_str, &prepared.path, false).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::Conflict);
    assert!(path.join("new.txt").exists(), "a refused archive must not touch files");

    remove_workspace(repo_str, &prepared.path, true).await.unwrap();
    assert!(!path.exists());
    assert!(!run(&repo, &["rev-parse", "--verify", "--quiet", "refs/heads/alder"]).is_empty());
}

#[test]
fn pull_requests_parse_from_what_gh_prints() {
    let json = br#"{"isDraft":true,"number":12,"state":"OPEN","title":"Fix login","url":"https://example.com/12"}"#;
    let pr = parse_pull_request(json).unwrap();
    assert_eq!((pr.number, pr.state.as_str(), pr.is_draft), (12, "OPEN", true));
    assert_eq!(parse_pull_request(b"not json"), None);
    assert_eq!(parse_pull_request(br#"{"state":"OPEN"}"#), None);
}

#[tokio::test]
async fn untracked_work_is_protected_even_if_git_is_set_to_hide_it() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    run(&repo, &["config", "status.showUntrackedFiles", "no"]);
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    std::fs::write(path.join("agent-output.txt"), "precious\n").unwrap();

    assert_eq!(status(&prepared.path).await.unwrap().files.len(), 1, "status must show it too");
    let err = remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::Conflict);
    assert!(path.join("agent-output.txt").exists());
}

#[tokio::test]
async fn commits_only_reachable_from_a_detached_head_are_protected() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    run(path, &["checkout", "-q", "--detach"]);
    std::fs::write(path.join("work.txt"), "work\n").unwrap();
    run(path, &["add", "."]);
    run(path, &["commit", "-qm", "on no branch"]);

    let err = remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.err().unwrap();
    assert_eq!(err.code, ErrorCode::Conflict, "{}", err.message);
    assert!(path.join("work.txt").exists());
    remove_workspace(repo.to_str().unwrap(), &prepared.path, true).await.unwrap();
}

#[tokio::test]
async fn a_detached_head_on_a_commit_a_branch_holds_can_be_archived() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    run(Path::new(&prepared.path), &["checkout", "-q", "--detach"]);
    remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.unwrap();
    assert!(!Path::new(&prepared.path).exists());
}

#[tokio::test]
async fn ignored_files_do_not_block_archiving() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let ignored = Path::new(&prepared.path).join("ignored");
    std::fs::create_dir_all(&ignored).unwrap();
    std::fs::write(ignored.join("cache"), "x").unwrap();
    remove_workspace(repo.to_str().unwrap(), &prepared.path, false).await.unwrap();
    assert!(!Path::new(&prepared.path).exists());
}

#[tokio::test]
async fn restoring_brings_back_committed_work() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    std::fs::write(path.join("kept.txt"), "kept\n").unwrap();
    run(path, &["add", "."]);
    run(path, &["commit", "-qm", "work"]);
    let repo_str = repo.to_str().unwrap();
    remove_workspace(repo_str, &prepared.path, false).await.unwrap();

    restore_workspace(repo_str, &prepared.path, "alder").await.unwrap();
    assert_eq!(std::fs::read_to_string(path.join("kept.txt")).unwrap(), "kept\n");
    assert_eq!(restore_workspace(repo_str, &prepared.path, "alder").await.err().unwrap().code, ErrorCode::Conflict);
}

#[tokio::test]
async fn a_worktree_deleted_by_hand_can_still_be_archived_and_restored() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    std::fs::remove_dir_all(&prepared.path).unwrap();
    let repo_str = repo.to_str().unwrap();
    remove_workspace(repo_str, &prepared.path, false).await.unwrap();
    restore_workspace(repo_str, &prepared.path, "alder").await.unwrap();
    assert!(Path::new(&prepared.path).join("a.txt").exists());
}

#[tokio::test]
async fn status_reports_each_kind_of_change_including_renames_and_odd_names() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    std::fs::write(path.join("a.txt"), "changed\n").unwrap();
    std::fs::write(path.join("with space.txt"), "x\n").unwrap();
    run(path, &["mv", ".gitignore", "renamed"]);

    let status = status(&prepared.path).await.unwrap();
    assert_eq!(status.branch.as_deref(), Some("alder"));
    assert!(status.head.is_some());
    let find = |p: &str| status.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("{p}: {:?}", status.files));
    assert_eq!(find("a.txt").status, ".M");
    assert_eq!(find("with space.txt").status, "??");
    assert_eq!(find("renamed").orig_path.as_deref(), Some(".gitignore"));
    assert!(find("renamed").status.starts_with('R'));
}

#[tokio::test]
async fn diff_covers_commits_uncommitted_and_untracked_work() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let path = Path::new(&prepared.path);
    std::fs::write(path.join("committed.txt"), "from a commit\n").unwrap();
    run(path, &["add", "."]);
    run(path, &["commit", "-qm", "work"]);
    std::fs::write(path.join("a.txt"), "uncommitted\n").unwrap();
    std::fs::write(path.join("untracked.txt"), "brand new\n").unwrap();
    // The base branch moving on must not show up as changes in this workspace.
    std::fs::write(repo.join("moved.txt"), "elsewhere\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "main moved"]);

    let diff = diff(&prepared.path, Some("main")).await.unwrap();
    assert!(diff.base_commit.is_some());
    assert!(diff.patch.contains("+from a commit"), "{}", diff.patch);
    assert!(diff.patch.contains("+uncommitted"), "{}", diff.patch);
    assert!(diff.patch.contains("+brand new"), "{}", diff.patch);
    assert!(!diff.patch.contains("elsewhere"), "{}", diff.patch);
    assert!(!diff.truncated);
}
