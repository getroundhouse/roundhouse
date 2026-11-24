//! A path the summary lists twice, as a file taken out of git's index and left in the worktree is: once as deleted,
//! and once as untracked.

#![cfg(test)]

use client_core::fake_hub::WorkerReads;
use gpui_kit::{AppContext as _, TestAppContext};
use protocol::model::{ChangeStatus, ChangedFile, ChangedFileAt, FileDiffBody, LineCounts};

use super::world::{current, page_with, summarized};

const DELETED: &str = "diff --git a/a.txt b/a.txt
deleted file mode 100644
index 5626abf..0000000
--- a/a.txt
+++ /dev/null
@@ -1 +0,0 @@
-one
";

const UNTRACKED: &str = "diff --git a/a.txt b/a.txt
new file mode 100644
index 0000000..814f4a4
--- /dev/null
+++ b/a.txt
@@ -0,0 +1,2 @@
+one
+kept
";

/// Each of the two rows of `a.txt` is a row of its own, and shows its own file's diff, which is asked for as the
/// summary names it: the deletion's, which removes its one line, and then the untracked file's, which adds two.
#[gpui_kit::test]
fn each_file_at_a_path_listed_twice_shows_its_own_diff(cx: &mut TestAppContext) {
    let listed = |status, lines: (u64, u64), version: Option<&str>| ChangedFile {
        path: "a.txt".into(),
        from: None,
        status,
        lines: Some(LineCounts { added: lines.0, removed: lines.1 }),
        version: version.map(|version| version.to_string().into()),
        name_not_utf8: false,
    };
    let files = vec![listed(ChangeStatus::Deleted, (0, 1), None), listed(ChangeStatus::Untracked, (2, 0), Some("v2"))];
    let mut reads = WorkerReads::default();
    for (untracked, patch, version) in [(false, DELETED, None), (true, UNTRACKED, Some("v2"))] {
        let page = page_with(FileDiffBody::Patch { patch: patch.into(), next_line: None }, version);
        reads.file_diffs.insert((ChangedFileAt { untracked, path: "a.txt".into() }, 0), vec![page]);
    }
    let read = reads.diffs_read.clone();
    let (_hub, mut ui, diff, _asked) = summarized(cx, (files, 2), reads, current);
    let drawn = |ui: &crate::support::Ui<'_>| {
        ui.cx.read_entity(&diff.file, |view, cx| view.file(cx).map(|file| (file.added(), file.removed())))
    };

    for (row, untracked, lines) in [("file-a.txt", false, (0, 1)), ("file-a.txt:untracked", true, (2, 0))] {
        ui.wait("its row", |ui| ui.has(row));
        ui.click(row);

        ui.wait(row, |ui| drawn(ui) == Some(lines));
        let asked = read.lock().last().map(|(at, _)| at.clone());
        assert_eq!(asked, Some(ChangedFileAt { untracked, path: "a.txt".into() }), "{row} asked for as listed");
    }
}
