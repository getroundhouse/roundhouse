//! What the composer is given to send: pasting, and the files waiting in it.

#![cfg(test)]

use gpui_kit::{AppContext as _, ClipboardItem};

use super::Ui;

impl Ui<'_> {
    /// Puts this on the clipboard and pastes it where focus is, as the platform's paste key does.
    pub(crate) fn paste(&mut self, item: ClipboardItem) {
        self.cx.write_to_clipboard(item);
        // The platform's own paste: cmd-v on a Mac, ctrl-v on Linux.
        self.press("secondary-v");
    }

    /// The names of the files waiting in the composer, in order.
    pub(crate) fn attached(&self) -> Vec<String> {
        let pane = self.pane().unwrap();
        self.cx.read_entity(&pane, |p, _| p.attached_names())
    }
}
