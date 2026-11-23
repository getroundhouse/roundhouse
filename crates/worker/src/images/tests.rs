use std::path::Path;

use protocol::attachment::MediaType;
use protocol::client::ErrorCode;

use super::read;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nrest";

fn refused(worktree: &Path, path: &str) -> ErrorCode {
    read(worktree, path, u64::MAX).expect_err(path).code
}

#[test]
fn an_image_in_the_worktree_reads_by_a_path_relative_to_it_or_in_full() {
    let dir = tempfile::tempdir().unwrap();
    let worktree = dir.path().join("worktree");
    std::fs::create_dir_all(worktree.join("docs")).unwrap();
    std::fs::write(worktree.join("docs/shot.png"), PNG).unwrap();

    let relative = read(&worktree, "docs/shot.png", u64::MAX).unwrap();
    let full = read(&worktree, worktree.join("docs/shot.png").to_str().unwrap(), u64::MAX).unwrap();

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
fn only_an_image_the_transcript_draws_or_a_video_it_plays_can_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let webm = protocol::fixtures::webm(1000);
    std::fs::write(dir.path().join("board.webm"), &webm).unwrap();
    std::fs::write(dir.path().join("board.mp4"), b"\0\0\0\x18ftypisom\0\0\x02\0").unwrap();
    std::fs::write(dir.path().join(".env"), "TOKEN=secret").unwrap();
    std::fs::write(dir.path().join("secret.png"), "TOKEN=secret").unwrap();

    assert_eq!(read(dir.path(), "board.webm", u64::MAX).unwrap(), (MediaType::Webm, webm));
    for path in [".env", "secret.png", "board.mp4"] {
        assert_eq!(refused(dir.path(), path), ErrorCode::InvalidRequest, "{path}");
    }
    assert_eq!(refused(dir.path(), "missing.png"), ErrorCode::NotFound);
}

/// A video can be as big as any file a session keeps, and an image a third of that, each read only as far as its limit.
#[test]
fn an_image_or_a_video_past_its_limit_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let webm = protocol::fixtures::WEBM_START;
    let sized = |name: &str, start: &[u8], len: u64| {
        let file = std::fs::File::create(dir.path().join(name)).unwrap();
        std::io::Write::write_all(&mut &file, start).unwrap();
        file.set_len(len).unwrap();
    };
    sized("long.webm", webm, protocol::attachment::MAX_FILE_BYTES);
    sized("too-long.webm", webm, protocol::attachment::MAX_FILE_BYTES + 1);
    sized("huge.png", PNG, protocol::attachment::MAX_FILE_BYTES);

    assert_eq!(read(dir.path(), "long.webm", u64::MAX).unwrap().0, MediaType::Webm);
    assert_eq!(refused(dir.path(), "too-long.webm"), ErrorCode::InvalidRequest);
    assert_eq!(refused(dir.path(), "huge.png"), ErrorCode::InvalidRequest);
}

/// One bigger than the room it's given is refused, read no further than that.
#[test]
fn a_file_bigger_than_its_room_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let webm = protocol::fixtures::webm(1000);
    std::fs::write(dir.path().join("board.webm"), &webm).unwrap();

    assert_eq!(read(dir.path(), "board.webm", 1000).unwrap().1, webm);
    assert_eq!(read(dir.path(), "board.webm", 999).unwrap_err().code, ErrorCode::InvalidRequest);
}
