//! Showing Settings in place of the sidebar and the workspace, and going back.

use gpui_kit::{AppContext as _, Context, Entity, FocusHandle, Window};

use crate::settings::{Asking, OfHub, PageId};

use super::AppView;

/// Settings is showing.
pub(super) struct Shown {
    /// What had focus before, to give it back after.
    focus_before: Option<FocusHandle>,
    /// The page chosen, which the sidebar of pages changes, and `None` for the first. Kept only while Settings shows,
    /// so it opens on its first page unless it's opened at another.
    pub(super) chosen: Entity<Option<PageId>>,
}

impl AppView {
    /// What Settings shows of the hub, and the hub it asks for a change.
    pub(super) fn settings_of_hub(&self) -> OfHub {
        let asking = self.client().zip(self.feed.clone()).map(|(client, feed)| Asking { client, feed });
        OfHub { records: self.records.clone(), asking }
    }

    pub(super) fn settings_open(&self) -> bool {
        self.settings.is_some()
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_at(None, window, cx);
    }

    /// Opens Settings at `page`, or on its first page. Settings already showing stays at the page it's on.
    pub(crate) fn open_settings_at(&mut self, page: Option<PageId>, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open() {
            return;
        }
        self.settings = Some(Shown { focus_before: window.focused(cx), chosen: cx.new(|_| page) });
        crate::sounds::read_list(cx);
        window.focus(&self.settings_focus, cx);
        // The chat showing is hidden, so a reply that comes now isn't read.
        self.read_on_screen(window.is_window_active(), cx);
        cx.notify();
    }

    /// Back to where the person was, with focus where they left it, such as in the composer. A workspace selected
    /// meanwhile, as when the one showing was archived elsewhere, has it instead, since what had it isn't drawn now.
    pub(crate) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(shown) = self.settings.take() else { return };
        // The pane must be marked visible before its focus guard can accept the returned focus.
        self.read_on_screen(window.is_window_active(), cx);
        match self.focus_once_closed.take().filter(|id| self.selected() == Some(id)) {
            Some(id) => self.focus_pane(id, window, cx),
            None => window.focus(&shown.focus_before.unwrap_or_else(|| self.focus.clone()), cx),
        }
        cx.notify();
    }
}
