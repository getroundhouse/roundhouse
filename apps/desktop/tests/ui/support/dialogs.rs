//! The dialog over the window, if there's one: whether one is open, and what it asks.

#![cfg(test)]

use gpui_kit::AppContext as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::test::TestWindowExt as _;

use super::Ui;

impl Ui<'_> {
    pub(crate) fn has_dialog(&mut self) -> bool {
        self.frame();
        self.cx.update_window(self.handle, |_, window, cx| window.has_active_dialog(cx)).unwrap()
    }

    /// What the dialog in front asks, and what it says under that, while one asks, such as before quitting.
    pub(crate) fn asked(&mut self) -> Option<(String, String)> {
        self.frame();
        self.cx
            .update_window(self.handle, |_, window, cx| {
                if !window.has_active_dialog(cx) {
                    return None;
                }
                let said = |id: &'static str| window.try_find(id).and_then(|e| e.label().map(str::to_string));
                Some((said("dialog-title")?, said("dialog-lead").unwrap_or_default()))
            })
            .unwrap()
    }
}
