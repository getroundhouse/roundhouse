//! What the window's tab keys do in a workspace, whatever kind of tab each is: `Ctrl-Tab` and `Ctrl-Shift-Tab` move
//! between its tabs, in the order they're drawn, and `Cmd-W` closes the one showing.

use gpui_kit::{Context, Window};
use protocol::model::{SessionId, TerminalId};

use super::{Showing, WorkspacePane};

/// A tab `Ctrl-Tab` comes to.
#[derive(Clone, PartialEq)]
enum Stop {
    File,
    Subagent,
    Proposal,
    Chat(SessionId),
    Terminal(TerminalId),
}

impl WorkspacePane {
    /// Moves to the tab before or after this one, in the order they're drawn, wrapping around: the file tab, the
    /// subagent tab, the proposal tab, the chats, then the terminals.
    pub fn step_chat(&mut self, step: isize, cx: &mut Context<Self>) {
        let chats = self.open_sessions().map(|session| Stop::Chat(session.id.clone()));
        let stops: Vec<Stop> = (self.file_tab().map(|_| Stop::File).into_iter())
            .chain(self.subagents.tab_session().map(|_| Stop::Subagent))
            .chain(self.proposals.tab_session().map(|_| Stop::Proposal))
            .chain(chats)
            .chain(self.shown.terminals().iter().map(|terminal| Stop::Terminal(terminal.id.clone())))
            .collect();
        let here = match (self.showing(), self.active.clone()) {
            (Showing::File, _) => Stop::File,
            (Showing::Subagent, _) => Stop::Subagent,
            (Showing::Proposal, _) => Stop::Proposal,
            (Showing::Terminal, _) => match self.terminal_in_front() {
                Some(id) => Stop::Terminal(id.clone()),
                None => return,
            },
            (Showing::Chat, Some(id)) => Stop::Chat(id),
            (Showing::Chat, None) => return,
        };
        let Some(current) = stops.iter().position(|stop| *stop == here) else { return };
        match stops[crate::app::wrapped_step(current, step, stops.len())].clone() {
            Stop::File => self.show_file_tab(cx),
            Stop::Subagent => self.bring_to_front(Showing::Subagent, cx),
            Stop::Proposal => self.bring_to_front(Showing::Proposal, cx),
            Stop::Chat(id) => self.activate(id, cx),
            Stop::Terminal(id) => self.show_terminal(id, cx),
        }
    }

    /// Closes whichever tab is showing, as `Cmd-W` does.
    pub fn close_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.showing() {
            Showing::Subagent => self.close_subagent_tab(cx),
            Showing::Proposal => self.close_proposal_tab(cx),
            Showing::File => self.close_file_tab(cx),
            Showing::Terminal | Showing::Chat => {
                if let Some(id) = self.terminal_in_front().cloned() {
                    self.close_terminal(id, cx);
                } else if let Some(id) = self.active.clone() {
                    self.close_chat(id, window, cx);
                }
            }
        }
    }
}
