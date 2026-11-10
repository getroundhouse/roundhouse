//! The steps that open files: a changed one from Changes, the list of All files, one from it, and a comment on a line.

use gpui_kit::component::diff::{DiffLinePosition, DiffSide};

use super::{Driver, selected_pane};

impl Driver {
    pub(super) fn open_file(&mut self, path: &str) {
        self.click(format!("file-{path}"));
        self.wait("the file tab to show it", |view, cx| {
            let file = selected_pane(view).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.file.clone()));
            file.is_some_and(|file| file.read(cx).file(cx).is_some_and(|file| file.path() == path))
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
                viewer.read(cx).path.as_deref() == Some(path) && viewer.read(cx).shown_text().is_some()
            })
        });
    }

    pub(super) fn start_comment(&mut self, line: usize) {
        let view = self.view.clone();
        let file = self.cx.update(|cx| {
            selected_pane(view.read(cx)).and_then(|pane| pane.read(cx).diff.as_ref().map(|d| d.file.clone()))
        });
        self.cx
            .update_window(self.handle, |_, window, cx| {
                file.unwrap().update(cx, |f, cx| {
                    let path = f.path.clone().unwrap();
                    f.start_comment(DiffLinePosition::new(path, DiffSide::Modified, line), window, cx);
                });
            })
            .unwrap();
        self.frame();
    }
}
