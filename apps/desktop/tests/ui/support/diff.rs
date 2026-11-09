//! Reviewing a workspace's diff.

#![cfg(test)]

use desktop::file_diff::FileDiff;
use desktop::review::Review;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::{AppContext as _, Entity, SharedString};

use super::Ui;

/// The selected workspace's diff, and what its diff tab shows.
#[derive(Clone)]
pub(crate) struct Diff {
    pub(crate) review: Entity<Review>,
    pub(crate) file: Entity<FileDiff>,
}

impl Ui<'_> {
    /// Opens the selected workspace's sidebar of changes, and waits for the diff to load.
    pub(crate) fn open_diff(&mut self) -> Diff {
        self.click("toggle-diff");
        let pane = self.pane().unwrap();
        self.wait("the diff", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
        let diff = self.cx.read_entity(&pane, |p, _| {
            p.diff.as_ref().map(|diff| Diff { review: diff.review.clone(), file: diff.file.clone() })
        });
        let diff = diff.unwrap();
        self.wait("the diff to load", |ui| ui.cx.read_entity(&diff.review, |r, _| r.files.is_some()));
        diff
    }

    /// Clicks a file in the sidebar of changes, and waits for the diff tab to show it.
    pub(crate) fn open_file(&mut self, diff: &Diff, path: &str) {
        self.click(SharedString::from(format!("file-{path}")));
        self.wait("the diff tab to show it", |ui| {
            ui.cx.read_entity(&diff.file, |f, cx| f.file(cx).is_some_and(|f| f.path() == path) && f.state.is_some())
        });
    }

    /// Starts a comment on a line on the diff's new side, as that line's add button would.
    pub(crate) fn start_comment(&mut self, diff: &Diff, path: &str, line: usize) {
        let position = DiffLinePosition::new(path, DiffSide::Modified, line);
        self.cx
            .update_window(self.handle, |_, window, cx| {
                diff.file.update(cx, |f, cx| f.start_comment(position, window, cx));
            })
            .unwrap();
        self.frame();
    }

    /// The comments the diff shows.
    pub(crate) fn comments(&self, diff: &Diff) -> Vec<protocol::model::DiffComment> {
        self.cx.read_entity(&diff.review, |r, _| r.comments.clone())
    }
}
