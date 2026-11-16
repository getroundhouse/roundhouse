//! What's in front under the tabs: the active chat, the file tab or the subagent tab. What's read and what has focus
//! follow it, and `focus.rs` says where focus goes.

use gpui_kit::base::TextSelection;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::Copy;
use gpui_kit::{Context, Window};

use super::{Opened, PaneEvent, Showing, WorkspacePane};

impl WorkspacePane {
    /// What's in front under the tabs.
    pub fn showing(&self) -> Showing {
        if self.subagents.in_front() {
            return Showing::Subagent;
        }
        let tab = self.diff.as_ref().and_then(|diff| diff.tab.as_ref());
        if tab.is_some_and(|tab| tab.in_front) { Showing::File } else { Showing::Chat }
    }

    /// Puts the active chat, the file tab or the subagent tab in front, as long as that tab is open. It's the only way
    /// any of them comes to the front, so what's read and what has focus always follow what's drawn.
    pub(super) fn bring_to_front(&mut self, front: Showing, cx: &mut Context<Self>) {
        if let Some(tab) = self.diff.as_mut().and_then(|diff| diff.tab.as_mut()) {
            tab.in_front = front == Showing::File;
        }
        self.subagents.set_in_front(front == Showing::Subagent);
        // The chat is read only while it's in front, and is dictated to only while its composer is.
        self.showing_changed(cx);
        cx.emit(PaneEvent::Showing);
        // Not behind a dialog, where typing meant for it would come here unseen. The app focuses the pane showing once
        // the dialog closes.
        self.with_window(cx, |t, window, cx| {
            if !window.has_active_dialog(cx) {
                t.focus_showing(window, cx);
            }
        });
        cx.notify();
    }

    /// Copies what's selected in what's in front, wherever focus is, once a text box with nothing selected lets the
    /// action go on. What the kit has selected, such as a command asked about in the composer's place, goes on to the
    /// window's root, which copies it.
    pub fn copy_showing(&self, window: &mut Window, cx: &mut Context<Self>) {
        if !TextSelection::selected_text(window, cx).trim().is_empty() {
            cx.propagate();
            return;
        }
        match self.showing() {
            Showing::Chat => match self.transcript() {
                Some(transcript) => transcript.update(cx, |t, cx| t.copy_selection(cx)),
                None => cx.propagate(),
            },
            Showing::Subagent => match self.subagents.transcript().cloned() {
                Some(transcript) => transcript.update(cx, |t, cx| t.copy_selection(cx)),
                None => cx.propagate(),
            },
            Showing::File => {
                match self.diff.as_ref().filter(|_| matches!(self.file_tab_opened(), Some(Opened::Diff) | None)) {
                    Some(diff) => diff.file.update(cx, |file, cx| file.copy(&Copy, window, cx)),
                    None => cx.propagate(),
                }
            }
        }
    }
}
