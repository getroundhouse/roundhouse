//! Reviewing a workspace's diff.

#![cfg(test)]

use desktop::diff_panel::DiffPanel;
use gpui_kit::component::diff::{DiffLinePosition, DiffSide};
use gpui_kit::{AppContext as _, Entity};

use super::Ui;

impl Ui<'_> {
    /// Opens the selected workspace's diff, and waits for it to load.
    pub(crate) fn open_diff(&mut self) -> Entity<DiffPanel> {
        self.click("toggle-diff");
        let pane = self.pane().unwrap();
        self.wait("the diff panel", |ui| ui.cx.read_entity(&pane, |p, _| p.diff.is_some()));
        let diff = self.cx.read_entity(&pane, |p, _| p.diff.clone()).unwrap();
        self.wait("the diff to load", |ui| ui.cx.read_entity(&diff, |d, _| d.state.is_some()));
        diff
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
