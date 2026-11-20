//! Where focus goes once nothing has it, as when what had it goes away: the composer microphone's card, once the
//! microphone dictates or is hidden, or what closing Settings gives focus back to, if that went meanwhile. The window's
//! keys reach nothing without it, as "Focus stays on something that's drawn" in `docs/standards/gpui.md` says.

use gpui_kit::{Context, Focusable as _, Window};

use super::AppView;

impl AppView {
    /// Focuses Settings while it shows, Home's search while it does, and what's in front in the workspace otherwise,
    /// as `WorkspacePane::focus_showing` has it. An open new-workspace dialog gets its prompt back; with another dialog
    /// open, or none of those views, the window itself gets focus so its keys still work.
    pub(super) fn focus_what_shows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        if self.settings_open() {
            window.focus(&self.settings_focus, cx);
        } else if look::dialog_open(window, cx) {
            // The retained draft isn't necessarily open. Only the visible prompt gets keys back when its microphone
            // card disappears; another dialog must never send typing into that hidden draft or the composer behind it.
            match self.new_workspace.as_ref().filter(|dialog| dialog.read(cx).is_shown()) {
                Some(dialog) => window.focus(&dialog.focus_handle(cx), cx),
                None => window.focus(&self.focus, cx),
            }
        } else if self.home_drawn() {
            self.home.search.update(cx, |search, cx| search.focus(window, cx));
        } else if let Some(pane) = self.pane_to_draw() {
            pane.update(cx, |pane, cx| pane.focus_showing(window, cx));
        } else {
            window.focus(&self.focus, cx);
        }
    }
}
