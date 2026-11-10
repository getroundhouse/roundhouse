use protocol::client::ErrorCode;
use protocol::model::FileContents;

use super::{MAX_BYTES, bounded, list, read, read_folder};
use crate::git::tests::{new_repo, run};

#[tokio::test]
async fn every_file_is_listed_in_order_ignored_ones_too() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::create_dir_all(repo.join("src/app")).unwrap();
    std::fs::write(repo.join("src/app/view.rs"), "fn view() {}\n").unwrap();
    std::fs::write(repo.join("src/lib.rs"), "mod app;\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "src"]);
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    std::fs::write(repo.join(".git/info/exclude"), "*.log\n").unwrap();
    std::fs::write(repo.join("src/debug.log"), "log\n").unwrap();
    std::fs::remove_file(repo.join("src/lib.rs")).unwrap();

    let files = list(&repo).await.unwrap();

    assert_eq!(
        files.paths,
        [".gitignore", "a.txt", "new.txt", "src/app/view.rs", "src/debug.log"],
        "not the deleted one"
    );
    assert!(files.folders.is_empty(), "{:?}", files.folders);
    assert!(!files.truncated);
}

/// A folder such as `target` can hold hundreds of thousands of files, so it's listed as a folder, to read when it's
/// opened, and so is a repository inside the worktree, which git lists as a folder.
#[tokio::test]
async fn a_folder_git_ignores_whole_or_another_repository_is_listed_as_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::create_dir_all(repo.join("ignored/deep")).unwrap();
    std::fs::write(repo.join("ignored/deep/build.log"), "log\n").unwrap();
    let inner = repo.join("vendor/inner");
    std::fs::create_dir_all(&inner).unwrap();
    run(&inner, &["init", "-q"]);
    std::fs::write(inner.join("x.txt"), "x\n").unwrap();

    let files = list(&repo).await.unwrap();

    assert_eq!(files.paths, [".gitignore", "a.txt"]);
    assert_eq!(files.folders, ["ignored", "vendor/inner"]);
}

#[tokio::test]
async fn a_folder_ignored_by_a_pattern_for_files_is_listed_once_as_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::write(repo.join(".git/info/exclude"), "*.log\n").unwrap();
    std::fs::create_dir_all(repo.join("logs")).unwrap();
    std::fs::create_dir_all(repo.join("deep/er")).unwrap();
    for log in ["logs/x.log", "logs/y.log", "deep/er/q.log"] {
        std::fs::write(repo.join(log), "log\n").unwrap();
    }

    let files = list(&repo).await.unwrap();

    // Git lists each of these folders, and then every file in it too.
    assert_eq!(files.paths, [".gitignore", "a.txt"]);
    assert_eq!(files.folders, ["deep", "logs"]);
}

#[tokio::test]
async fn a_submodule_and_a_link_to_a_folder_git_tracks_are_listed_as_folders() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let sub = dir.path().join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    run(&sub, &["init", "-q"]);
    std::fs::write(sub.join("s.txt"), "s\n").unwrap();
    run(&sub, &["add", "."]);
    run(&sub, &["commit", "-qm", "sub"]);
    run(&repo, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", sub.to_str().unwrap(), "vendor/sub"]);
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(repo.join("docs/a.md"), "a\n").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("docs", repo.join("documents")).unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "sub and link"]);

    let files = list(&repo).await.unwrap();

    assert_eq!(files.paths, [".gitignore", ".gitmodules", "a.txt", "docs/a.md"]);
    let folders: &[&str] = if cfg!(unix) { &["documents", "vendor/sub"] } else { &["vendor/sub"] };
    assert_eq!(files.folders, folders);
}

/// Git lists a link as a file whatever it leads to, so a new or ignored one that leads to a folder is listed as a folder,
/// as one git tracks is, rather than as a file that won't open.
#[cfg(unix)]
#[tokio::test]
async fn a_new_or_ignored_link_to_a_folder_is_listed_as_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    std::fs::write(repo.join("docs/a.md"), "a\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "docs"]);
    std::fs::write(repo.join(".git/info/exclude"), "venv\n").unwrap();
    std::os::unix::fs::symlink("docs", repo.join("documents")).unwrap();
    std::os::unix::fs::symlink("docs", repo.join("venv")).unwrap();
    std::os::unix::fs::symlink("a.txt", repo.join("notes.txt")).unwrap();

    let files = list(&repo).await.unwrap();

    assert_eq!(files.paths, [".gitignore", "a.txt", "docs/a.md", "notes.txt"], "a link to a file is still a file");
    assert_eq!(files.folders, ["documents", "venv"]);
}

#[test]
fn past_the_limit_ignored_files_are_left_out_first_and_then_new_ones() {
    let paths = |names: &[&str]| names.iter().map(ToString::to_string).collect::<Vec<_>>();

    let roomy = bounded([paths(&["b", "a"]), paths(&[]), paths(&["c"])], 3);
    let full = bounded([paths(&["b", "a"]), paths(&[]), paths(&["0-ignored", "z-ignored"])], 3);
    let over = bounded([paths(&["c", "b", "a"]), paths(&[]), paths(&["0-ignored"])], 2);
    let new = bounded([paths(&["m"]), paths(&["a-new", "z-new"]), paths(&["0-ignored"])], 2);

    assert_eq!(roomy, (paths(&["a", "b", "c"]), false));
    assert_eq!(full, (paths(&["0-ignored", "a", "b"]), true), "the files git tracks stay");
    assert_eq!(over, (paths(&["a", "b"]), true));
    assert_eq!(new, (paths(&["a-new", "m"]), true), "and stay before new ones, which stay before ignored ones");
}

#[test]
fn a_folder_reads_as_what_s_directly_in_it() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("target");
    std::fs::create_dir_all(target.join("debug/deps")).unwrap();
    std::fs::create_dir_all(target.join(".git")).unwrap();
    std::fs::write(target.join("debug/app"), "binary").unwrap();
    std::fs::write(target.join("CACHEDIR.TAG"), "tag").unwrap();
    std::os::unix::fs::symlink(target.join("debug"), target.join("latest")).unwrap();

    let files = read_folder(dir.path(), "target").unwrap();

    assert_eq!(files.paths, ["target/CACHEDIR.TAG"]);
    assert_eq!(files.folders, ["target/debug", "target/latest"], "a link to a folder too, and no .git");
    assert_eq!(read_folder(dir.path(), "target/debug").unwrap().folders, ["target/debug/deps"]);
}

#[test]
fn a_folder_outside_the_worktree_isn_t_read() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::os::unix::fs::symlink(dir.path(), worktree.join("up")).unwrap();

    assert_eq!(read_folder(&worktree, "up").unwrap_err().code, ErrorCode::InvalidRequest);
    assert_eq!(read_folder(&worktree, "..").unwrap_err().code, ErrorCode::InvalidRequest);
}

#[test]
fn a_file_reads_as_its_text() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.md"), "one\ntwo\n").unwrap();
    std::fs::write(dir.path().join("latin.txt"), b"caf\xe9\n").unwrap();

    assert_eq!(read(dir.path(), "notes.md").unwrap(), FileContents::Text { text: "one\ntwo\n".to_string() });
    assert_eq!(read(dir.path(), "latin.txt").unwrap(), FileContents::Text { text: "caf\u{fffd}\n".to_string() });
}

#[test]
fn a_file_that_isn_t_text_or_is_too_big_says_so_rather_than_sending_it() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("shot.png"), b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
    let big = std::fs::File::create(dir.path().join("big.log")).unwrap();
    big.set_len(MAX_BYTES + 1).unwrap();

    assert_eq!(read(dir.path(), "shot.png").unwrap(), FileContents::Binary);
    assert_eq!(read(dir.path(), "big.log").unwrap(), FileContents::TooBig { bytes: MAX_BYTES + 1 });
}

#[test]
fn a_file_as_big_as_the_limit_is_sent() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("full.txt"), "a".repeat(usize::try_from(MAX_BYTES).unwrap())).unwrap();

    let read = read(dir.path(), "full.txt").unwrap();

    assert!(matches!(read, FileContents::Text { text } if text.len() as u64 == MAX_BYTES));
}

#[test]
fn a_path_no_list_would_give_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(dir.path().join("src/a.txt"), "a").unwrap();
    let absolute = dir.path().join("src/a.txt");

    for path in
        [absolute.to_str().unwrap(), "./src/a.txt", "src/../src/a.txt", "src/./a.txt", "src//a.txt", "src/a.txt/", ""]
    {
        assert_eq!(read(dir.path(), path).unwrap_err().code, ErrorCode::InvalidRequest, "{path}");
    }
    for folder in [dir.path().join("src").to_str().unwrap(), ".", "./src", "src/", "src/.", ""] {
        assert_eq!(read_folder(dir.path(), folder).unwrap_err().code, ErrorCode::InvalidRequest, "{folder}");
    }
}

/// The one example of the worktree's own check, which `worktree_file` tests in full: a link that leads out of it.
#[cfg(unix)]
#[test]
fn nothing_outside_the_worktree_is_read() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(dir.path().join("outside.txt"), "secret").unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.txt"), worktree.join("inside.txt")).unwrap();

    let refused = read(&worktree, "inside.txt").unwrap_err();

    assert_eq!(refused.code, ErrorCode::InvalidRequest);
}
