//! The pages of the files a summary leaves out, read with a bound of two files a page, so a few files go past it.

use super::super::ChangeSummaries;
use crate::git::tests::{new_repo, run};
use crate::testing::given;
use protocol::model::{ChangedFile, ChangedFileAt, ChangedFilesPage};
use std::path::Path;

/// The most a summary or a page lists here.
const BOUND: usize = 2;

/// Where each of `files` is in a summary's order.
fn at(files: &[ChangedFile]) -> Vec<ChangedFileAt> {
    files.iter().map(ChangedFile::at).collect()
}

fn tracked(path: &str) -> ChangedFileAt {
    ChangedFileAt { untracked: false, path: path.into() }
}

fn untracked(path: &str) -> ChangedFileAt {
    ChangedFileAt { untracked: true, path: path.into() }
}

/// The page after `after`.
async fn page(summaries: &ChangeSummaries, worktree: &Path, after: &ChangedFileAt) -> ChangedFilesPage {
    summaries.page_after(&given(), worktree, None, after).await.unwrap()
}

/// A repository whose worktree changes two tracked files and adds three untracked ones.
fn changing_five(dir: &Path) -> std::path::PathBuf {
    let repo = new_repo(dir);
    std::fs::write(repo.join("b.txt"), "two\n").unwrap();
    run(&repo, &["add", "b.txt"]);
    run(&repo, &["commit", "-qm", "b"]);
    std::fs::write(repo.join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.join("b.txt"), "changed\n").unwrap();
    for name in ["u1.txt", "u2.txt", "u3.txt"] {
        std::fs::write(repo.join(name), "new\n").unwrap();
    }
    repo
}

/// The files past those a summary lists are read a page at a time, each after the last file of the one before, in the
/// summary's order, against its base, until a page leaves none out.
#[tokio::test]
async fn changed_files_past_a_summary_are_read_a_page_at_a_time_to_the_last() {
    let dir = tempfile::tempdir().unwrap();
    let repo = changing_five(dir.path());
    let summaries = ChangeSummaries::listing_at_most(BOUND);

    let summary = summaries.summarize(&given(), &repo, None).await.unwrap();
    let first = page(&summaries, &repo, &summary.files[BOUND - 1].at()).await;
    let last = page(&summaries, &repo, &first.files[BOUND - 1].at()).await;

    assert_eq!((at(&summary.files), summary.left_out), (vec![tracked("a.txt"), tracked("b.txt")], 3));
    assert_eq!((at(&first.files), first.left_out), (vec![untracked("u1.txt"), untracked("u2.txt")], 1));
    assert_eq!((at(&last.files), last.left_out), (vec![untracked("u3.txt")], 0), "the last says so");
    assert_eq!((&first.base_commit, &last.base_commit), (&summary.base_commit, &summary.base_commit));
    let past_the_last = page(&summaries, &repo, &untracked("u3.txt")).await;
    assert_eq!((past_the_last.files, past_the_last.left_out), (Vec::new(), 0));
}

/// A page starts after a file, not at a count, so a file added or removed ahead of where it starts shifts nothing
/// after, and one added behind it, which a client reads once the summary that has it comes, is in no page after.
#[tokio::test]
async fn changed_files_added_or_removed_between_two_pages_shift_nothing_after_where_the_next_starts() {
    let dir = tempfile::tempdir().unwrap();
    let repo = changing_five(dir.path());
    let summaries = ChangeSummaries::listing_at_most(BOUND);
    let first = page(&summaries, &repo, &tracked("b.txt")).await;
    let after = first.files[BOUND - 1].at();

    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    std::fs::remove_file(repo.join("u1.txt")).unwrap();
    std::fs::write(repo.join("t0.txt"), "behind\n").unwrap();
    std::fs::write(repo.join("u4.txt"), "after\n").unwrap();
    let next = page(&summaries, &repo, &after).await;

    assert_eq!(after, untracked("u2.txt"));
    assert_eq!((at(&next.files), next.left_out), (vec![untracked("u3.txt"), untracked("u4.txt")], 0));
    let summary = summaries.summarize(&given(), &repo, None).await.unwrap();
    assert_eq!(at(&summary.files), [tracked("b.txt"), untracked("t0.txt")], "a new summary has what moved behind");
}
