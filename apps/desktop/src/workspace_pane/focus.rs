//! Where focus goes in a workspace: to what's in front under the tabs, and in a chat, to what's in the composer's
//! place, which is the question the agent is waiting on while there is one. Focus has to stay on something drawn,
//! since the window's keys, such as `Ctrl-Tab`, reach nothing from what isn't.

use gpui_kit::{Context, FocusHandle, Focusable as _, Window};

use super::{Opened, Showing, WorkspacePane};

impl WorkspacePane {
    /// Focuses the file tab's view while it shows, and what's in the composer's place otherwise. The composer isn't
    /// drawn behind the file tab.
    pub fn focus_showing(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.showing() {
            Showing::File => {
                let Some(diff) = &self.diff else { return };
                match diff.tab.as_ref().map(|tab| tab.opened) {
                    Some(Opened::File) => window.focus(&diff.viewer.focus_handle(cx), cx),
                    Some(Opened::Diff) | None => window.focus(&diff.file.focus_handle(cx), cx),
                }
            }
            Showing::Chat => self.focus_composer(window, cx),
        }
    }

    /// Focuses what's in the composer's place: the question the agent is waiting on, or else the composer.
    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.transcript().and_then(|t| t.read(cx).question().cloned()) {
            Some(question) => window.focus(&question.focus_handle(cx), cx),
            None => self.composer.update(cx, |c, cx| c.focus(window, cx)),
        }
    }

    /// Gives the question focus that's been put back on the composer behind it, such as by closing Settings or a
    /// dialog, since the window's keys don't reach anything from what isn't drawn.
    pub(super) fn keep_focus_drawn(&self, window: &mut Window, cx: &mut Context<Self>) {
        let question = self.transcript().and_then(|t| t.read(cx).question().cloned());
        if let Some(question) = question.filter(|_| self.showing() == Showing::Chat)
            && self.composer.focus_handle(cx).is_focused(window)
        {
            window.focus(&question.focus_handle(cx), cx);
        }
    }

    /// Moves focus that was in the composer's place to what's there now, once a question comes or goes. Focus
    /// anywhere else stays where it is, and it can only be in the composer's place in the pane showing.
    pub(super) fn follow_question(&self, left: Option<&FocusHandle>, window: &mut Window, cx: &mut Context<Self>) {
        let was_there = self.composer.focus_handle(cx).contains_focused(window, cx)
            || left.is_some_and(|left| left.contains_focused(window, cx));
        if was_there && self.showing() == Showing::Chat {
            self.focus_composer(window, cx);
        }
    }
}
