//! Where focus goes once nothing has it, as when what had it goes away: the composer microphone's card, once the
//! microphone dictates or is hidden, or what closing Settings gives focus back to, if that went meanwhile. The window's
//! keys reach nothing without it, as "Focus stays on something that's drawn" in `docs/gpui.md` says.

use gpui_kit::component::WindowExt as _;
use gpui_kit::{Context, Window};

use super::AppView;

impl AppView {
    /// Focuses Settings while it shows, Home's search while it does, and what's in front in the workspace otherwise,
    /// as `WorkspacePane::focus_showing` has it. With a dialog open, or none of them, the window itself has focus, so
    /// its keys still work.
    pub(super) fn focus_what_shows(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_open() {
            window.focus(&self.settings_focus, cx);
        } else if window.has_active_dialog(cx) {
            // Not what's behind it, where typing meant for the dialog would go unseen.
            window.focus(&self.focus, cx);
        } else if self.home_drawn() {
            self.home.search.update(cx, |search, cx| search.focus(window, cx));
        } else if let Some(pane) = self.pane_to_draw() {
            pane.update(cx, |pane, cx| pane.focus_showing(window, cx));
        } else {
            window.focus(&self.focus, cx);
        }
    }
}
