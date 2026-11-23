//! One file's diff, taken from real repositories with a git that reads none of the person's config. The patches are
//! checked by the lines they add and remove, and by what git says of a rename, which every git prints the same.

use super::diff_file;
use super::diffing::of_one_version;
use super::pages::{Page, read_page};
use crate::git::ChangeSummaries;
use crate::git::tests::{new_repo, run};
use crate::testing::given;
use protocol::client::{ApiError, ErrorCode};
use protocol::model::{ChangedFile, ChangedFileAt, FileDiff, FileDiffBody, MOST_PATCH_PAGE_BYTES};
use protocol::worker::DiffFile;
use std::path::Path;

/// The tracked entry at `file`, or the untracked one where git tracks nothing there, as a request names either that
/// doesn't say which.
async fn diff(worktree: &Path, file: &str, from: Option<&str>, first_line: u32) -> Result<FileDiff, ApiError> {
    diff_of(worktree, (file, false), from, first_line).await
}

/// The entry at `file` that's untracked, or not, as `untracked` says.
async fn diff_of(
    worktree: &Path,
    (file, untracked): (&str, bool),
    from: Option<&str>,
    first_line: u32,
) -> Result<FileDiff, ApiError> {
    let asked = DiffFile {
        path: worktree.to_str().unwrap().into(),
        base_ref: None,
        file: file.into(),
        untracked,
        from: from.map(str::to_string),
        first_line,
    };
    diff_file(&given(), &asked).await
}

/// What a page's patch should hold and lack, or that it's binary or unchanged.
#[derive(Debug)]
enum Expected {
    Patch { holds: &'static [&'static str], lacks: &'static [&'static str] },
    Binary,
    Unchanged,
}

fn write(worktree: &Path, file: &str, content: impl AsRef<[u8]>) {
    std::fs::write(worktree.join(file), content).unwrap();
}

/// A repository in `dir` with each kind of change a summary names, and a change put back.
fn changing_each_kind(dir: &Path) -> std::path::PathBuf {
    let repo = new_repo(dir);
    for file in ["old.txt", "gone.txt"] {
        write(&repo, file, "one\ntwo\nthree\nfour\nfive\n");
    }
    write(&repo, "deleted.txt", "doomed\n");
    write(&repo, "unindexed.txt", "kept on disk\n");
    write(&repo, "put_back.txt", "as it was\n");
    write(&repo, "big.txt", "a line of a file larger than git reads to compare\n".repeat(40));
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "more"]);
    run(&repo, &["config", "core.bigFileThreshold", "1k"]);
    write(&repo, "a.txt", "uno\ntwo\n");
    write(&repo, "untracked.txt", "new\n");
    write(&repo, "untracked.bin", b"\0\x01\x02\n");
    write(&repo, "big.txt", "a line of a file larger than git reads to compare, changed\n".repeat(40));
    run(&repo, &["mv", "old.txt", "new.txt"]);
    write(&repo, "new.txt", "one\ntwo\nthree\nfour\nfive\nsix\n");
    run(&repo, &["mv", "gone.txt", "rewritten.txt"]);
    write(&repo, "rewritten.txt", "nothing like it\n");
    std::fs::remove_file(repo.join("deleted.txt")).unwrap();
    run(&repo, &["rm", "-q", "--cached", "unindexed.txt"]);
    write(&repo, "put_back.txt", "other\n");
    write(&repo, "put_back.txt", "as it was\n");
    std::fs::create_dir(repo.join("ignored")).unwrap();
    write(&repo, "ignored/build.log", "ignored\n");
    repo
}

/// Each kind of change a summary names, each entry asked for as the summary names it, and what's been put back since,
/// with the version the summary gives each and the base it's taken against. Not what git ignores. A file taken out of
/// git's index and left in the worktree is two entries, a deletion and an untracked file, and each is read apart. An
/// untracked file asked for without saying so is read as a request from before `untracked` asks for it.
#[tokio::test]
async fn diff_file_takes_each_kind_of_change_against_the_summary_s_base_with_its_version() {
    let dir = tempfile::tempdir().unwrap();
    let repo = changing_each_kind(dir.path());
    let summary = ChangeSummaries::default().summarize(&given(), &repo, None).await.unwrap();
    let patch = |holds, lacks| Expected::Patch { holds, lacks };
    let cases = [
        (("a.txt", false), None, patch(&["-one\n", "+uno\n", "+two\n"], &[])),
        (("untracked.txt", true), None, patch(&["+new\n"], &[])),
        (("untracked.txt", false), None, patch(&["+new\n"], &[])),
        (("untracked.bin", true), None, Expected::Binary),
        (("big.txt", false), None, Expected::Binary),
        (("new.txt", false), Some("old.txt"), patch(&["rename from old.txt\n", "+six\n"], &[])),
        (("rewritten.txt", false), Some("gone.txt"), patch(&["+nothing like it\n"], &["gone.txt", "-one\n"])),
        (("deleted.txt", false), None, patch(&["-doomed\n"], &[])),
        (("unindexed.txt", false), None, patch(&["-kept on disk\n"], &["+kept on disk\n"])),
        (("unindexed.txt", true), None, patch(&["+kept on disk\n"], &["-kept on disk\n"])),
        (("put_back.txt", false), None, Expected::Unchanged),
        (("ignored/build.log", true), None, Expected::Unchanged),
        (("never_there.txt", false), None, Expected::Unchanged),
    ];

    let pair = summary.files.iter().filter(|listed| listed.path == "unindexed.txt").map(ChangedFile::at);
    assert_eq!(pair.map(|at| at.untracked).collect::<Vec<_>>(), [false, true], "listed twice");

    for ((file, untracked), from, expected) in cases {
        let diff = diff_of(&repo, (file, untracked), from, 0).await.unwrap();

        assert_eq!(diff.base_commit, summary.base_commit, "{file}");
        let deletion =
            ["deleted.txt", "never_there.txt"].contains(&file) || (file, untracked) == ("unindexed.txt", false);
        assert_eq!(diff.version.is_some(), !deletion, "{file}, untracked {untracked}: {diff:?}");
        let at = ChangedFileAt { untracked, path: file.into() };
        if let Some(listed) = summary.files.iter().find(|listed| listed.at() == at) {
            assert_eq!(diff.version, listed.version, "{file}, untracked {untracked}: as the summary names it");
        }
        match (&expected, &diff.body) {
            (Expected::Patch { holds, lacks }, FileDiffBody::Patch { patch, next_line: None }) => {
                assert!(holds.iter().all(|line| patch.contains(line)), "{file} holds {holds:?}: {patch}");
                assert!(!lacks.iter().any(|line| patch.contains(line)), "{file} lacks {lacks:?}: {patch}");
            }
            (Expected::Binary, FileDiffBody::Binary) | (Expected::Unchanged, FileDiffBody::Unchanged) => {}
            _ => panic!("{file}: expected {expected:?}, got {diff:?}"),
        }
    }
}

/// A link is diffed as itself, as git tracks one, so its version is the summary's, of the path it leads to, and a file
/// reached through a link to a folder outside the worktree, which git lists nothing of, has no version.
#[cfg(unix)]
#[tokio::test]
async fn diff_file_takes_a_link_as_itself_and_reads_nothing_through_one_out_of_the_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    write(dir.path(), "outside.txt", "secret\n");
    std::os::unix::fs::symlink(dir.path().join("outside.txt"), repo.join("link.txt")).unwrap();
    std::os::unix::fs::symlink(dir.path(), repo.join("up")).unwrap();
    let summary = ChangeSummaries::default().summarize(&given(), &repo, None).await.unwrap();

    let link = diff_of(&repo, ("link.txt", true), None, 0).await.unwrap();
    let through_a_link = diff(&repo, "up/outside.txt", None, 0).await.unwrap();

    let listed = summary.files.iter().find(|listed| listed.path == "link.txt").expect("the link is listed");
    assert!(link.version.is_some(), "{link:?}");
    assert_eq!(link.version, listed.version, "as the summary names it");
    assert_eq!((through_a_link.version, through_a_link.body), (None, FileDiffBody::Unchanged));
}

/// A path that names nothing in the worktree, as one going up a folder or from the root does, is refused rather than
/// handed to git, which would read a file anywhere as one it doesn't track, and so is one no summary would name, as
/// `worktree_file::relative` says.
#[tokio::test]
async fn diff_file_refuses_a_path_from_outside_the_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let outside = dir.path().join("outside.txt");
    write(dir.path(), "outside.txt", "secret\n");

    let asked = [("../outside.txt", None), (outside.to_str().unwrap(), None), ("a.txt", Some("../a.txt")), ("", None)];
    for (file, from) in asked.into_iter().chain([("./a.txt", None), ("a.txt", Some("a//b.txt"))]) {
        let refused = diff(&repo, file, from, 0).await.unwrap_err();

        assert_eq!(refused.code, ErrorCode::InvalidRequest, "{file}, from {from:?}");
    }
}

/// A new file of more lines than a page holds comes in two pages, the first as full as whole lines let it be, which
/// together are the whole patch.
#[tokio::test]
async fn diff_file_reads_a_patch_over_a_page_in_pages_of_whole_lines() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    let lines = MOST_PATCH_PAGE_BYTES / 10;
    let content = (0..lines).map(|n| format!("{n:012}")).collect::<Vec<_>>().join("\n") + "\n";
    write(&repo, "large.txt", &content);

    let first = diff(&repo, "large.txt", None, 0).await.unwrap();
    let FileDiffBody::Patch { patch: first_page, next_line: Some(next_line) } = &first.body else {
        panic!("{first:?}")
    };
    let second = diff(&repo, "large.txt", None, *next_line).await.unwrap();
    let FileDiffBody::Patch { patch: second_page, next_line: None } = &second.body else { panic!("{second:?}") };

    assert!(first_page.len() <= MOST_PATCH_PAGE_BYTES && first_page.len() > MOST_PATCH_PAGE_BYTES - 14);
    assert_eq!(first_page.lines().count(), usize::try_from(*next_line).unwrap(), "whole lines");
    let added: String = [first_page.as_str(), second_page]
        .concat()
        .lines()
        .filter_map(|line| line.strip_prefix('+').filter(|line| !line.starts_with("++")))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    assert_eq!(added, content);
    assert_eq!(first.version, second.version);
}

/// A file written while it's diffed has no version, since the patch may be of neither.
#[tokio::test]
async fn diff_file_gives_no_version_of_a_file_that_changed_while_it_was_diffed() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    write(&repo, "a.txt", "two\n");

    let (still, ()) = of_one_version(&repo, "a.txt", async { Ok(()) }).await.unwrap();
    let (changed, ()) = of_one_version(&repo, "a.txt", async {
        write(&repo, "a.txt", "three\n");
        Ok(())
    })
    .await
    .unwrap();

    assert!(still.is_some());
    assert_eq!(changed, None);
}

/// A page is whole lines while they fit, and a line longer than a page is one alone, cut to fit with a newline. Pages
/// count lines from the first, and one past the end is empty and the last.
#[tokio::test]
async fn diff_file_pages_are_whole_lines_and_a_line_longer_than_a_page_is_cut_to_one() {
    let patch = b"one\ntwo\nthree is long\nfour\n";
    let page = async |first_line, most| read_page(&mut &patch[..], first_line, most).await.unwrap();
    let paged = |patch: &str, next_line| Page { patch: patch.into(), next_line };

    assert_eq!(page(0, 100).await, paged("one\ntwo\nthree is long\nfour\n", None));
    assert_eq!(page(0, 9).await, paged("one\ntwo\n", Some(2)), "as many whole lines as fit");
    assert_eq!(page(2, 9).await, paged("three is\n", Some(3)), "cut, with its newline");
    assert_eq!(page(3, 9).await, paged("four\n", None));
    assert_eq!(page(1, 4).await, paged("two\n", Some(2)), "exactly full");
    assert_eq!(page(4, 9).await, paged("", None), "past the end");
}

/// A line cut where a character's bytes would be split is cut before the character, so the page is still UTF-8.
#[tokio::test]
async fn diff_file_cuts_a_long_line_between_characters() {
    let patch = "+caf\u{e9}\u{e9}\u{e9}\n".as_bytes();

    let page = read_page(&mut &patch[..], 0, 6).await.unwrap();

    assert_eq!(page, Page { patch: "+caf\n".into(), next_line: None });
}

/// A name with a character git would take as a wildcard, which only Unix lets a file have, names only that file.
#[cfg(unix)]
#[tokio::test]
async fn diff_file_takes_a_wildcard_in_a_name_as_itself() {
    let dir = tempfile::tempdir().unwrap();
    let repo = new_repo(dir.path());
    for file in ["x*.txt", "xy.txt"] {
        write(&repo, file, "one\n");
    }
    run(&repo, &["add", "."]);
    run(&repo, &["commit", "-qm", "more"]);
    for file in ["x*.txt", "xy.txt"] {
        write(&repo, file, "two\n");
    }

    let diff = diff(&repo, "x*.txt", None, 0).await.unwrap();

    let FileDiffBody::Patch { patch, .. } = diff.body else { panic!("{diff:?}") };
    assert!(patch.contains("x*.txt") && !patch.contains("xy.txt"), "{patch}");
}
