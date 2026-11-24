//! Reviewing a workspace's diff.

#![cfg(test)]

use std::cell::Cell;
use std::rc::Rc;

use desktop::file_diff::{FileDiff, LineAt};
use desktop::review::{Review, ReviewEvent};
use desktop::sidebar_tabs::SidebarTab;
use desktop::workspace_pane::WorkspacePane;
use gpui_kit::{App, AppContext as _, Entity, SharedString};
use protocol::model::DiffSide;

use super::Ui;

/// The selected workspace's diff, and what its file tab shows.
#[derive(Clone)]
pub(crate) struct Diff {
    pub(crate) review: Entity<Review>,
    pub(crate) file: Entity<FileDiff>,
}

/// Whether the sidebar of changes shows what it starts on: the changes, or all the files where there are none.
pub(crate) fn sidebar_loaded(pane: &WorkspacePane, cx: &App) -> bool {
    pane.diff.as_ref().is_some_and(|diff| match pane.sidebar_tab() {
        Some(SidebarTab::Changes) => true,
        Some(SidebarTab::AllFiles) => diff.all_files.read(cx).is_listed(),
        None => false,
    })
}

impl Ui<'_> {
    /// Whether the sidebar of changes shows, which it does in every workspace or none.
    pub(crate) fn changes_open(&self) -> bool {
        self.cx.read_entity(&self.pane().unwrap(), WorkspacePane::changes_open)
    }

    /// Shows the selected workspace's sidebar of changes, hiding it first if it shows, and waits for it to load the
    /// diff as it is now. It's open from the start, so what's changed since needs loading again to show, as hiding
    /// and showing it does.
    pub(crate) fn open_changes(&mut self) -> Diff {
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
        // While the hub keeps the changed files current, Changes lists its summary of them, and nothing reads the
        // whole diff.
        let summarized = |ui: &Ui| ui.cx.read_entity(&diff.review, |r, _| r.listing().is_some());
        match before {
            Some(_) => self.wait("the diff to load again", |ui| loaded.get() || summarized(ui)),
            None => self.wait("the diff to load", |ui| ui.cx.read_entity(&diff.review, |r, _| r.listing().is_some())),
        }
        diff
    }

    /// Clicks a file in Changes, and waits for the file tab to show its diff, made with the file as it was, or for a
    /// file that isn't text, to start comparing its sides.
    pub(crate) fn open_diff_of(&mut self, diff: &Diff, path: &str) {
        // Listed once the summary of the changes has it, which follows a file written just before.
        let row = SharedString::from(format!("file-{path}"));
        self.wait("its row in Changes", |ui| ui.has(row.clone()));
        self.click(row);
        self.wait("the file tab to show it", |ui| {
            ui.cx.read_entity(&diff.file, |view, cx| {
                view.file(cx).is_some_and(|file| file.path() == path && (file.is_binary() || view.is_ready()))
            })
        });
    }

    /// Starts a comment on a line on the diff's new side, as that line's add button would.
    pub(crate) fn start_comment(&mut self, diff: &Diff, path: &str, line: u32) {
        let position = LineAt { path: path.into(), side: DiffSide::New, line };
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
