//! Showing Settings in place of the sidebar and the workspace, and going back.

use gpui_kit::{Context, FocusHandle, Window};

use super::AppView;

/// Settings is showing.
pub(super) struct Shown {
    /// What had focus before, to give it back after.
    focus_before: Option<FocusHandle>,
}

impl AppView {
    pub(super) fn settings_open(&self) -> bool {
        self.settings.is_some()
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open() {
            return;
        }
        self.settings = Some(Shown { focus_before: window.focused(cx) });
        window.focus(&self.settings_focus, cx);
        // The chat showing is hidden, so a reply that comes now isn't read.
        self.read_on_screen(window.is_window_active(), cx);
        cx.notify();
    }

    /// Back to where the person was, with focus where they left it, such as in the composer.
    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.settings.take() else { return };
        window.focus(&shown.focus_before.unwrap_or_else(|| self.focus.clone()), cx);
        self.read_on_screen(window.is_window_active(), cx);
        cx.notify();
    }
}
