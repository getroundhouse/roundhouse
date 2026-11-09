//! Reviewing a workspace's diff.

#![cfg(test)]

use desktop::diff_panel::DiffPanel;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::{AppContext as _, Entity, SharedString};

use super::Ui;

impl Ui<'_> {
    /// Opens the selected workspace's sidebar of changes, and waits for the diff to load.
    pub(crate) fn open_diff(&mut self) -> Entity<DiffPanel> {
        self.click("toggle-diff");
        let pane = self.pane().unwrap();
        self.wait("the diff", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
        let diff = self.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
        self.wait("the diff to load", |ui| ui.cx.read_entity(&diff, |d, _| d.files.is_some()));
        diff
    }

    /// Clicks a file in the sidebar of changes, and waits for the diff tab to show it.
    pub(crate) fn open_file(&mut self, diff: &Entity<DiffPanel>, path: &str) {
        self.click(SharedString::from(format!("file-{path}")));
        self.wait("the diff tab to show it", |ui| {
            ui.cx.read_entity(diff, |d, _| d.open_file().is_some_and(|f| f.path() == path) && d.state.is_some())
        });
    }

    /// Starts a comment on a line on the diff's new side, as that line's add button would.
    pub(crate) fn start_comment(&mut self, diff: &Entity<DiffPanel>, path: &str, line: usize) {
        let position = DiffLinePosition::new(path, DiffSide::Modified, line);
        self.cx
            .update_window(self.handle, |_, window, cx| diff.update(cx, |d, cx| d.start_comment(position, window, cx)))
            .unwrap();
        self.frame();
    }
}
