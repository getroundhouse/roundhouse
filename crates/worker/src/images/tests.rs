use std::path::Path;

use protocol::attachment::MediaType;
use protocol::client::ErrorCode;

use super::read;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

fn refused(worktree: &Path, path: &str) -> ErrorCode {
    read(worktree, path).expect_err(path).code
}

#[test]
fn an_image_in_the_worktree_reads_by_a_path_relative_to_it_or_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(worktree.join("docs")).unwrap();
    std::fs::write(worktree.join("docs/shot.png"), PNG).unwrap();

    let relative = read(&worktree, "docs/shot.png").unwrap();
    let full = read(&worktree, worktree.join("docs/shot.png").to_str().unwrap()).unwrap();
    let wandering = read(&worktree, "docs/../docs/./shot.png").unwrap();

    assert_eq!(relative, (MediaType::Png, PNG.to_vec()));
    assert_eq!(full, relative);
    assert_eq!(wandering, relative);
}

#[test]
fn nothing_outside_the_worktree_can_be_read_even_through_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(dir.path().join("outside.png"), PNG).unwrap();
    // A sibling whose name starts with the worktree's, which a check of the path as text would let through.
    std::fs::create_dir_all(dir.path().join("worktree-2")).unwrap();
    std::fs::write(dir.path().join("worktree-2/shot.png"), PNG).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.png"), worktree.join("link.png")).unwrap();
    std::os::unix::fs::symlink(dir.path(), worktree.join("up")).unwrap();

    let sibling = dir.path().join("worktree-2/shot.png");
    let outside = dir.path().join("outside.png");
    for path in ["../outside.png", outside.to_str().unwrap(), "link.png", "up/outside.png", sibling.to_str().unwrap()] {
        assert_eq!(refused(&worktree, path), ErrorCode::InvalidRequest, "{path}");
    }
}

#[test]
fn only_an_image_the_transcript_draws_can_be_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), "TOKEN=secret").unwrap();
    std::fs::write(dir.path().join("secret.png"), "TOKEN=secret").unwrap();
    std::fs::create_dir(dir.path().join("folder.png")).unwrap();

    for path in [".env", "secret.png", "folder.png", "."] {
        assert_eq!(refused(dir.path(), path), ErrorCode::InvalidRequest, "{path}");
    }
    assert_eq!(refused(dir.path(), "missing.png"), ErrorCode::NotFound);
}
