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

    assert_eq!(relative, (MediaType::Png, PNG.to_vec()));
    assert_eq!(full, relative);
}

/// What the agent's text points to is untrusted, so the worktree's own check comes first.
#[test]
fn an_image_outside_the_worktree_can_t_be_read_through_a_link() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(&worktree).unwrap();
    std::fs::write(dir.path().join("outside.png"), PNG).unwrap();
    std::os::unix::fs::symlink(dir.path().join("outside.png"), worktree.join("link.png")).unwrap();

    assert_eq!(refused(&worktree, "link.png"), ErrorCode::InvalidRequest);
}

#[test]
fn only_an_image_the_transcript_draws_can_be_read() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".env"), "TOKEN=secret").unwrap();
    std::fs::write(dir.path().join("secret.png"), "TOKEN=secret").unwrap();

    for path in [".env", "secret.png"] {
        assert_eq!(refused(dir.path(), path), ErrorCode::InvalidRequest, "{path}");
    }
    assert_eq!(refused(dir.path(), "missing.png"), ErrorCode::NotFound);
}
