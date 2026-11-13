//! The workspace's terminals: a tab for each open one, after the chats', and the view each shows, made the first time
//! it shows and kept while it's open, so it stays current and keeps its place. Closing a tab ends the shell, and a
//! shell that ends, however it does, takes its tab with it, since the hub drops the terminal's record.

use std::collections::HashMap;

use client_core::{Client, OpenTerminal};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::{
    AnyElement, AppContext as _, Context, ElementId, Entity, Focusable as _, IntoElement as _, ParentElement as _,
    Styled, Window, div,
};
use look::{InRegion as _, Region};
use protocol::model::{Record, Terminal, TerminalId};
use protocol::terminal::TerminalSize;
use terminal_view::TerminalView;

use super::{Next, PaneEvent, Showing, WorkspacePane, tab};

/// The size a terminal opens at, which its view changes to the size it has room for as soon as it's drawn.
const OPENING_SIZE: TerminalSize = TerminalSize { cols: 80, rows: 24 };

#[derive(Default)]
pub(super) struct Terminals {
    /// The one in front under the tabs, while one is.
    in_front: Option<TerminalId>,
    /// The view of each one shown so far, while it's open.
    views: HashMap<TerminalId, Entity<TerminalView>>,
    /// One is on its way, so another press doesn't open a second.
    opening: bool,
}

impl Terminals {
    pub(super) fn set_in_front(&mut self, in_front: Option<TerminalId>) {
        self.in_front = in_front;
    }
}

impl WorkspacePane {
    fn open_terminals(&self) -> &[Terminal] {
        self.shown.terminals()
    }

    /// The terminal in front under the tabs, as long as it's still open.
    pub(super) fn terminal_in_front(&self) -> Option<&TerminalId> {
        let id = self.terminals.in_front.as_ref()?;
        self.open_terminals().iter().any(|open| &open.id == id).then_some(id)
    }

    fn view_in_front(&self) -> Option<&Entity<TerminalView>> {
        self.terminal_in_front().and_then(|id| self.terminals.views.get(id))
    }

    /// Opens a terminal in the workspace's worktree, and shows it once client-core's copy of the records has it.
    pub fn new_terminal(&mut self, cx: &mut Context<Self>) {
        if std::mem::replace(&mut self.terminals.opening, true) {
            return;
        }
        let (workspace, id) = (self.workspace().id.clone(), client_core::new_terminal_id());
        let opening = self.client.start(move |client| client.terminal_open(&workspace, &id, OPENING_SIZE));
        cx.spawn(async move |this, cx| {
            let opened = opening.recv().await;
            let _ = this.update(cx, |t, cx| {
                t.terminals.opening = false;
                match opened {
                    Ok(terminal) => {
                        // As a new chat's is, so it shows once client-core's copy has it, from the reply or the feed,
                        // whichever comes first.
                        t.records.take_reply(Record::Terminal(terminal.clone()));
                        t.show_terminal(terminal.id, cx);
                    }
                    Err(e) => cx.emit(PaneEvent::Error(format!("Couldn't open a terminal: {e}"))),
                }
            });
        })
        .detach();
    }

    /// Brings the terminal to the front, with a view made for it the first time it shows, or once the records have it.
    pub(super) fn show_terminal(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        if !self.open_terminals().iter().any(|open| open.id == id) {
            self.showing_next = Some(Next::Terminal(id));
            return;
        }
        if self.terminals.views.contains_key(&id) {
            self.bring_terminal_to_front(id, cx);
            return;
        }
        self.with_window(cx, move |t, window, cx| {
            // Closed before this ran, which would make a view of a terminal gone.
            if !t.open_terminals().iter().any(|open| open.id == id) {
                return;
            }
            // Asked for again before this ran, which made its view, so it isn't made twice, but it comes to the front,
            // since another may have been asked for between.
            if !t.terminals.views.contains_key(&id) {
                let open = OpenTerminal::start(&t.client, &id);
                let view = cx.new(|cx| TerminalView::new(open, window, cx));
                t.terminals.views.insert(id.clone(), view);
            }
            t.bring_terminal_to_front(id, cx);
        });
    }

    fn bring_terminal_to_front(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        self.terminals.in_front = Some(id);
        self.bring_to_front(Showing::Terminal, cx);
    }

    /// Ends the terminal's shell, which closes it, and shows the chat if it was in front.
    pub(super) fn close_terminal(&mut self, id: TerminalId, cx: &mut Context<Self>) {
        let was_in_front = self.terminal_in_front() == Some(&id);
        self.terminals.views.remove(&id);
        let closing = self.client.start(move |client| client.terminal_close(&id));
        cx.spawn(async move |this, cx| match closing.recv().await {
            Ok(_) => {}
            // Its shell ended on its own as it was closed, so it's gone either way.
            Err(e) if e.is_not_found() => {}
            Err(e) => {
                let _ = this.update(cx, |_, cx| cx.emit(PaneEvent::Error(format!("Couldn't close the terminal: {e}"))));
            }
        })
        .detach();
        if was_in_front {
            self.show_chat(cx);
        }
    }

    /// Lets go of the views of terminals that have closed, here or elsewhere, and shows the chat in place of one that
    /// was in front.
    pub(super) fn set_terminals(&mut self, cx: &mut Context<Self>) {
        let open: Vec<TerminalId> = self.open_terminals().iter().map(|terminal| terminal.id.clone()).collect();
        self.terminals.views.retain(|id, _| open.contains(id));
        if self.terminals.in_front.is_some() && self.terminal_in_front().is_none() {
            self.show_chat(cx);
        }
    }

    /// Carries each terminal shown on to a new connection, which starts it again from what it shows.
    pub(super) fn reconnect_terminals(&self, client: &Client, cx: &mut Context<Self>) {
        for view in self.terminals.views.values() {
            view.read(cx).open_terminal().reconnect(client);
        }
    }

    pub(super) fn focus_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(view) = self.view_in_front() {
            window.focus(&view.focus_handle(cx), cx);
        }
    }

    /// The view of the terminal in front.
    #[cfg(feature = "testing")]
    pub fn terminal(&self) -> Option<Entity<TerminalView>> {
        self.view_in_front().cloned()
    }

    /// The terminal in front, under the tabs, in place of the chat.
    pub(super) fn terminal_view(&self) -> Option<AnyElement> {
        let inset = div().size_full().pl_3().pt_2().child(self.view_in_front()?.clone());
        Some(div().region(Region::Terminal).flex_1().min_h_0().w_full().child(inset).into_any_element())
    }

    /// A tab for each open terminal, in the order they were opened, numbered by its place among them after the first.
    pub(super) fn terminal_tabs(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ids: Vec<TerminalId> = self.open_terminals().iter().map(|terminal| terminal.id.clone()).collect();
        (1..).zip(ids).map(|(number, id)| self.terminal_tab(&id, number, cx)).collect()
    }

    fn terminal_tab(&self, id: &TerminalId, number: usize, cx: &mut Context<Self>) -> AnyElement {
        let name = if number == 1 { "Terminal".to_owned() } else { format!("Terminal {number}") };
        let selected = self.showing() == Showing::Terminal && self.terminal_in_front() == Some(id);
        let (to_show, to_close) = (id.clone(), id.clone());
        let close_id = ElementId::Name(format!("close-terminal-{id}").into());
        let close = tab::close_button(close_id, "Close terminal", cx, move |t, _, cx| {
            t.close_terminal(to_close.clone(), cx);
        });
        let face = tab::face(ElementId::Name(format!("terminal-{id}").into()), selected, name.clone())
            // Muted whether or not it's showing, as the chats' titles are: the underline says which tab is.
            .text_color(cx.theme().muted_foreground)
            .on_click(cx.listener(move |t, _, _, cx| t.show_terminal(to_show.clone(), cx)))
            .child(Icon::new(Lucide::Terminal).xsmall())
            .child(tab::name().child(name));
        tab::tab(ElementId::Name(format!("terminal-tab-{id}").into()), face, close, selected).into_any_element()
    }
}
