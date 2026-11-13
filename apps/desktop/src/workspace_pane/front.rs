//! What's in front under the tabs: the active chat, the file tab, the subagent tab or a terminal. What's read and what
//! has focus follow it, and `focus.rs` says where focus goes.

use gpui_kit::Context;

use super::{Next, PaneEvent, Showing, WorkspacePane};

impl WorkspacePane {
    /// What's in front under the tabs.
    pub fn showing(&self) -> Showing {
        if self.subagents.in_front() {
            return Showing::Subagent;
        }
        let tab = self.diff.as_ref().and_then(|diff| diff.tab.as_ref());
        if tab.is_some_and(|tab| tab.in_front) {
            Showing::File
        } else if self.terminal_in_front().is_some() {
            Showing::Terminal
        } else {
            Showing::Chat
        }
    }

    /// Puts the active chat, the file tab, the subagent tab or the terminal last chosen in front, as long as that tab
    /// is open. It's the only way any of them comes to the front, so what's read and what has focus always follow
    /// what's drawn. The app moves focus to what's in front, and only in the pane on screen, since only it knows which
    /// that is, and a pane behind it can change what it has in front too, such as when a terminal in it ends.
    pub(super) fn bring_to_front(&mut self, front: Showing, cx: &mut Context<Self>) {
        if let Some(tab) = self.diff.as_mut().and_then(|diff| diff.tab.as_mut()) {
            tab.in_front = front == Showing::File;
        }
        self.subagents.set_in_front(front == Showing::Subagent);
        if front != Showing::Terminal {
            self.terminals.set_in_front(None);
        }
        // The chat is read only while it's in front.
        self.read_showing(cx);
        cx.emit(PaneEvent::Showing);
        cx.notify();
    }

    /// Shows the chat or terminal waiting to be shown, once client-core's copy has it open.
    pub(super) fn show_next(&mut self, cx: &mut Context<Self>) {
        let ready = match &self.showing_next {
            Some(Next::Chat(id)) => self.open_sessions().any(|s| &s.id == id),
            Some(Next::Terminal(id)) => self.shown.terminals().iter().any(|open| &open.id == id),
            None => false,
        };
        match self.showing_next.take_if(|_| ready) {
            Some(Next::Chat(id)) => self.activate(id, cx),
            Some(Next::Terminal(id)) => self.show_terminal(id, cx),
            None => {}
        }
    }
}
