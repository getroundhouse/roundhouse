//! Summaries of real repositories' changes.

use super::super::ChangeSummaries;
use crate::git::tests::{new_repo, prepare, run};
use crate::testing::given;
use protocol::model::{ChangeStatus, ChangeSummary, CommitId, FileVersion, LineCounts};
use std::fs::File;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Each file's path, the path it was renamed from, how it changed and its lines.
type Row<'a> = (&'a str, Option<&'a str>, ChangeStatus, Option<(u64, u64)>);

fn rows(summary: &ChangeSummary) -> Vec<Row<'_>> {
    let lines = |lines: Option<LineCounts>| lines.map(|lines| (lines.added, lines.removed));
    summary
        .files
        .iter()
        .map(|file| (file.path.as_str(), file.from.as_deref(), file.status, lines(file.lines)))
        .collect()
}

fn version<'a>(summary: &'a ChangeSummary, path: &str) -> Option<&'a FileVersion> {
    let file = summary.files.iter().find(|file| file.path == path);
    file.unwrap_or_else(|| panic!("{path} isn't in {summary:?}")).version.as_ref()
}

async fn summarize(summaries: &ChangeSummaries, worktree: &Path, base_ref: Option<&str>) -> ChangeSummary {
    summaries.summarize(&given(), worktree, base_ref).await.unwrap()
}

/// What a commit on the workspace's branch did and what's not committed yet, against where the branch left its base,
/// though the base has moved on since. Not what git ignores.
#[tokio::test]
async fn a_summary_has_every_change_against_the_merge_base_and_its_lines_are_their_sums() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::write(repo.join("gone.txt"), "one\ntwo\n").unwrap();
    std::fs::write(repo.join("old.txt"), "moves\nwith\nme\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "more"]);
    let prepared = prepare(&dir.path().join("ws"), &repo, "alder", None).await.unwrap();
    let worktree = Path::new(&prepared.path);
    let merge_base = CommitId::from(run(worktree, &["rev-parse", "HEAD"]));
    std::fs::write(worktree.join("committed.txt"), "from a commit\n").unwrap();
    run(worktree, &["add", "."]);
    run(worktree, &["commit", "-qm", "work"]);
    std::fs::write(worktree.join("a.txt"), "one\nand two\n").unwrap();
    run(worktree, &["rm", "-q", "gone.txt"]);
    run(worktree, &["mv", "old.txt", "new.txt"]);
    std::fs::write(worktree.join("untracked.txt"), "no\nnewline at the end").unwrap();
    std::fs::write(worktree.join("untracked.bin"), b"\0\x01\x02\n").unwrap();
    std::fs::create_dir(worktree.join("ignored")).unwrap();
    std::fs::write(worktree.join("ignored/build.log"), "ignored\n").unwrap();
    std::fs::write(repo.join("moved.txt"), "elsewhere\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "main moved"]);

    let summary = summarize(&ChangeSummaries::default(), worktree, Some("main")).await;

    assert_eq!(summary.base_commit, merge_base);
    assert_eq!(
        rows(&summary),
        [
            ("a.txt", None, ChangeStatus::Modified, Some((1, 0))),
            ("committed.txt", None, ChangeStatus::Added, Some((1, 0))),
            ("gone.txt", None, ChangeStatus::Deleted, Some((0, 2))),
            ("new.txt", Some("old.txt"), ChangeStatus::Renamed, Some((0, 0))),
            ("untracked.bin", None, ChangeStatus::Untracked, None),
            ("untracked.txt", None, ChangeStatus::Untracked, Some((2, 0))),
        ]
    );
    assert_eq!(summary.lines, LineCounts { added: 4, removed: 2 });
    assert_eq!(version(&summary, "gone.txt"), None, "nothing to read");
    let blob = run(worktree, &["hash-object", "--no-filters", "untracked.txt"]);
    assert_eq!(version(&summary, "untracked.txt"), Some(&FileVersion::from(blob)), "as git hashes the file");
}

/// Whether a file has changed is told by its size and modified time, and a time too recent to tell by, which a write
/// in the same tick of the filesystem's clock can leave as it was. Then by its content, so a file only touched keeps
/// its version.
#[tokio::test]
async fn a_file_s_version_changes_with_its_content_and_is_read_again_only_once_its_size_or_time_does() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let file = repo.join("a.txt");
    let summaries = ChangeSummaries::default();
    let write = |text: &str, modified: Option<SystemTime>| {
        std::fs::write(&file, text).unwrap();
        if let Some(modified) = modified {
            File::options().write(true).open(&file).unwrap().set_modified(modified).unwrap();
        }
    };
    let ago = |minutes: u64| Some(SystemTime::now() - Duration::from_secs(minutes * 60));
    let (an_hour_ago, half_an_hour_ago, twenty_minutes_ago) = (ago(60), ago(30), ago(20));
    let summary = async || summarize(&summaries, &repo, None).await;
    write("first\n", an_hour_ago);

    let first = summary().await;
    let again = summary().await;
    write("first\n", half_an_hour_ago);
    let touched = summary().await;
    write("other\n", twenty_minutes_ago);
    let rewritten = summary().await;
    write("stale\n", twenty_minutes_ago);
    let unread = summary().await;
    write("fresh\n", None);
    let recent = summary().await;
    let just_now = std::fs::metadata(&file).unwrap().modified().ok();
    write("again\n", just_now);
    let same_tick = summary().await;

    let head = CommitId::from(run(&repo, &["rev-parse", "HEAD"]));
    assert_eq!(first.base_commit, head, "with no base ref, against HEAD");
    let of = |summary: &ChangeSummary| version(summary, "a.txt").cloned().unwrap();
    assert_eq!(of(&again), of(&first), "untouched");
    assert_eq!(of(&touched), of(&first), "only touched");
    assert_ne!(of(&rewritten), of(&first), "the same size, with other content");
    assert_eq!(of(&unread), of(&rewritten), "the same size and time, so not read again");
    assert_ne!(of(&same_tick), of(&recent), "the same size and time, but too recent to tell by");
}

/// A base ref git finds no merge base with, as one that's gone, gives what HEAD's commit has, as a diff does.
#[tokio::test]
async fn a_summary_against_a_base_ref_that_s_gone_is_against_head() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();

    let summary = summarize(&ChangeSummaries::default(), &repo, Some("gone")).await;

    assert_eq!(summary.base_commit, CommitId::from(run(&repo, &["rev-parse", "HEAD"])));
    assert_eq!(rows(&summary), [("a.txt", None, ChangeStatus::Modified, Some((1, 0)))]);
}

/// A name with a byte that isn't UTF-8, which Linux lets a file have, and macOS and Windows don't, so it's tested only
/// on Linux. The file is counted and versioned as its bytes name it, named with the byte replaced, and marked, as a file
/// renamed from such a name is, since neither can be asked for by the name sent.
#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_file_whose_name_isn_t_utf_8_is_read_by_its_bytes_and_marked() {
    use std::os::unix::ffi::OsStrExt as _;
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let named = |name: &[u8]| repo.join(std::ffi::OsStr::from_bytes(name));
    std::fs::write(named(b"tracked\xe9.txt"), "one\n").unwrap();
    std::fs::write(named(b"was\xe9.txt"), "moves\nwith\nme\n").unwrap();
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "names"]);
    std::fs::write(named(b"tracked\xe9.txt"), "one\ntwo\n").unwrap();
    std::fs::rename(named(b"was\xe9.txt"), repo.join("now.txt")).unwrap();
    run(&repo, &["add", "--all"]);
    std::fs::write(named(b"caf\xe9.txt"), "one\n").unwrap();
    std::fs::write(repo.join("a.txt"), "one\nand two\n").unwrap();

    let summary = summarize(&ChangeSummaries::default(), &repo, None).await;

    assert_eq!(
        rows(&summary),
        [
            ("a.txt", None, ChangeStatus::Modified, Some((1, 0))),
            ("now.txt", Some("was\u{fffd}.txt"), ChangeStatus::Renamed, Some((0, 0))),
            ("tracked\u{fffd}.txt", None, ChangeStatus::Modified, Some((1, 0))),
            ("caf\u{fffd}.txt", None, ChangeStatus::Untracked, Some((1, 0))),
        ]
    );
    let marked: Vec<_> = summary.files.iter().map(|file| file.name_not_utf8).collect();
    assert_eq!(marked, [false, true, true, true]);
    assert!(version(&summary, "caf\u{fffd}.txt").is_some());
    assert!(version(&summary, "tracked\u{fffd}.txt").is_some());
}
