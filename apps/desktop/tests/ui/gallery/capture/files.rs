//! The steps that open files: a changed one from Changes, the list of All files, one from it, and a comment on a line.

use desktop::file_diff::LineAt;
use protocol::model::DiffSide;

use super::{Driver, selected_pane};

impl Driver {
    pub(super) fn open_file(&mut self, path: &str) {
        self.click(format!("file-{path}"));
        self.wait("the file tab to show it", |view, cx| {
            let file = selected_pane(view).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.file.clone()));
            // A changed image has no lines, and its comparison says when it's ready.
            file.is_some_and(|file| {
                let (view, file) = (file.read(cx), file.read(cx).file(cx));
                file.is_some_and(|file| file.path() == path && (file.is_binary() || view.is_ready()))
            })
        });
    }

    pub(super) fn all_files(&mut self) {
        self.click("sidebar-tab-all-files");
        self.wait("All files to list them", |view, cx| {
            let all_files =
                selected_pane(view).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.all_files.clone()));
            all_files.is_some_and(|all_files| all_files.read(cx).is_listed())
        });
    }

    pub(super) fn view_file(&mut self, path: &str) {
        self.click(format!("all-file-{path}"));
        self.wait("the file tab to show it", |view, cx| {
            let viewer = selected_pane(view).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.viewer.clone()));
            viewer.is_some_and(|viewer| {
                let viewer = viewer.read(cx);
                viewer.path.as_deref() == Some(path)
                    && (viewer.shown_text().is_some() || viewer.shown_image().is_some())
            })
        });
    }

    pub(super) fn start_comment(&mut self, line: u32) {
        let view = self.view.clone();
        let file = self.cx.update(|cx| {
            selected_pane(view.read(cx)).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.file.clone()))
        });
        self.cx
            .update_window(self.handle, |_, window, cx| {
                file.unwrap().update(cx, |f, cx| {
                    let path = f.path.clone().unwrap();
                    f.start_comment(LineAt { path, side: DiffSide::New, line }, window, cx);
                });
            })
            .unwrap();
        self.frame();
    }
}
