use std::path::Path;

use protocol::client::ErrorCode;

use super::{find, find_folder};

fn refused(worktree: &Path, path: &str) -> ErrorCode {
    find(worktree, path).expect_err(path).code
}

#[test]
fn a_file_in_the_worktree_is_found_by_a_path_relative_to_it_or_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(worktree.join("docs")).unwrap();
    std::fs::write(worktree.join("docs/notes.md"), "notes").unwrap();
    let expected = worktree.join("docs/notes.md").canonicalize().unwrap();

    let relative = find(&worktree, "docs/notes.md").unwrap();
    let full = find(&worktree, expected.to_str().unwrap()).unwrap();
    let wandering = find(&worktree, "docs/../docs/./notes.md").unwrap();

    assert_eq!((relative, full, wandering), (expected.clone(), expected.clone(), expected));
}

#[test]
fn nothing_outside_the_worktree_is_found_even_through_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(dir.path().join("outside.txt"), "secret").unwrap();
    // A sibling whose name starts with the worktree's, which a check of the path as text would let through.
    std::fs::create_dir_all(dir.path().join("worktree-2")).unwrap();
    std::fs::write(dir.path().join("worktree-2/notes.txt"), "secret").unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.txt"), worktree.join("link.txt")).unwrap();
    std::os::unix::fs::symlink(dir.path(), worktree.join("up")).unwrap();

    let sibling = dir.path().join("worktree-2/notes.txt");
    let outside = dir.path().join("outside.txt");
    for path in ["../outside.txt", outside.to_str().unwrap(), "link.txt", "up/outside.txt", sibling.to_str().unwrap()] {
        assert_eq!(refused(&worktree, path), ErrorCode::InvalidRequest, "{path}");
    }
}

#[test]
fn only_a_file_is_found() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("folder.txt")).unwrap();

    for path in ["folder.txt", "."] {
        assert_eq!(refused(dir.path(), path), ErrorCode::InvalidRequest, "{path}");
    }
    assert_eq!(refused(dir.path(), "missing.txt"), ErrorCode::NotFound);
}

#[test]
fn a_folder_is_found_as_a_file_is_and_only_inside_the_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(worktree.join("target/debug")).unwrap();
    std::fs::write(worktree.join("notes.txt"), "notes").unwrap();
    std::os::unix::fs::symlink(dir.path(), worktree.join("up")).unwrap();

    let found = find_folder(&worktree, "target/debug").unwrap();

    assert_eq!(found, worktree.join("target/debug").canonicalize().unwrap());
    assert_eq!(find_folder(&worktree, "up").unwrap_err().code, ErrorCode::InvalidRequest, "a link out of it");
    assert_eq!(find_folder(&worktree, "notes.txt").unwrap_err().code, ErrorCode::InvalidRequest, "a file");
    assert_eq!(find_folder(&worktree, "missing").unwrap_err().code, ErrorCode::NotFound);
}

#[test]
fn a_path_through_a_folder_that_s_now_a_file_isn_t_there() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("was-a-folder"), "now a file").unwrap();

    assert_eq!(refused(dir.path(), "was-a-folder/inside.txt"), ErrorCode::NotFound);
}

/// A name longer than any file's can be is a request no list would make, rather than the worker failing.
#[test]
fn a_name_too_long_to_be_one_is_refused() {
    let dir = tempfile::tempdir().unwrap();

    assert_eq!(refused(dir.path(), &"a".repeat(300)), ErrorCode::InvalidRequest);
}
