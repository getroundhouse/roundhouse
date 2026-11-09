//! Reviewing a workspace's diff.

#![cfg(test)]

use std::cell::Cell;
use std::rc::Rc;

use desktop::file_diff::FileDiff;
use desktop::review::{Review, ReviewEvent};
use desktop::workspace_pane::WorkspacePane;
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
    /// Whether the sidebar of changes shows, which it does in every workspace or none.
    pub(crate) fn changes_open(&self) -> bool {
        self.cx.read_entity(&self.pane().unwrap(), WorkspacePane::changes_open)
    }

    /// Shows the selected workspace's sidebar of changes, hiding it first if it shows, and waits for it to load the
    /// diff as it is now. It's open from the start, so what's changed since needs loading again to show, as hiding
    /// and showing it does.
    pub(crate) fn open_diff(&mut self) -> Diff {
        let pane = self.pane().unwrap();
        if self.changes_open() {
            self.click("toggle-diff");
        }
        let before = self.cx.read_entity(&pane, |p, _| p.diff.as_ref().map(|diff| diff.review.clone()));
        let loaded = Rc::new(Cell::new(false));
        let _watching = before.as_ref().map(|review| {
            let loaded = loaded.clone();
            self.cx.update(|cx| {
                cx.subscribe(review, move |_, event: &ReviewEvent, _| {
                    if matches!(event, ReviewEvent::Loaded) {
                        loaded.set(true);
                    }
                })
            })
        });

        self.click("toggle-diff");

        self.wait("the diff", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
        let diff = self.cx.read_entity(&pane, |p, _| {
            p.diff.as_ref().map(|diff| Diff { review: diff.review.clone(), file: diff.file.clone() })
        });
        let diff = diff.unwrap();
        // A diff made just now loads what's there now. One made before loads again, and only that load is new.
        match before {
            Some(_) => self.wait("the diff to load again", |_| loaded.get()),
            None => self.wait("the diff to load", |ui| ui.cx.read_entity(&diff.review, |r, _| r.files.is_some())),
        }
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
