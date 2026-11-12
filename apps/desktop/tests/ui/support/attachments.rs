//! What the composer is given to send: pasting, and the files waiting in it.

#![cfg(test)]

use gpui_kit::{AppContext as _, ClipboardEntry, ClipboardItem, ExternalPaths};

use super::Ui;

impl Ui<'_> {
    /// Puts this on the clipboard and pastes it where focus is, as the platform's paste key does.
    pub(crate) fn paste(&mut self, item: ClipboardItem) {
        self.cx.write_to_clipboard(item);
        // The platform's own paste: cmd-v on a Mac, ctrl-v on Linux.
        self.press("secondary-v");
    }

    /// Copies a file with this in it and pastes it in the composer, and waits for it to be attached.
    pub(crate) fn attach_a_file(&mut self, name: &str, text: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, text).unwrap();
        let before = self.attached().len();
        self.paste(ClipboardItem::from(ClipboardEntry::ExternalPaths(ExternalPaths(vec![path].into()))));
        self.wait("the file to be attached", |ui| ui.attached().len() > before);
    }

    /// Takes a file out of the composer, as its tile's remove button does. The button shows only under the pointer,
    /// over a tile the tests can't find.
    pub(crate) fn remove_attached(&mut self, id: &protocol::model::AttachmentId) {
        let pane = self.pane().unwrap();
        self.cx.update_entity(&pane, |p, cx| p.click_remove(id, cx));
        self.frame();
    }

    /// The names of the files waiting in the composer, in order.
    pub(crate) fn attached(&self) -> Vec<String> {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| p.attached_names())
    }
}
