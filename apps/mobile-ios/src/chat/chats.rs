//! The workspace's other chats, from the menu on the chat's title, where the desktop has a tab for each: each open one,
//! checked where it's the one shown, then starting another, then those closed, to reopen, most recently closed first,
//! with how long ago.

use protocol::client::sessions::{SessionCreate, SessionReopen};
use std::cmp::Reverse;

use client_core::refusals::couldnt;
use gpui_kit::assets::IconName;
use gpui_kit::{App, Context, Pixels, WeakEntity, Window, px};
use look::{Menu, NoteKind, Space};
use protocol::model::{Session, SessionId};

use super::over::Over;
use super::view::ChatView;

/// Room for a title of a few words beside how long ago, which the screen's width leaves on any phone, so a long one is
/// cut short rather than the menu running off the screen, as the desktop's menu of closed chats is.
const WIDTH: Pixels = px(320.);

/// The menu, built each time it opens, from the workspace's chats as the records then say.
pub(super) fn chats_menu(this: WeakEntity<ChatView>) -> impl Fn(Menu, &mut Window, &mut App) -> Menu + 'static {
    move |menu, window, cx| {
        let Some(chat) = this.upgrade() else { return menu };
        let mut menu = menu.width(WIDTH.min(window.viewport_size().width - Space::Medium.size() * 2.));
        let (records, workspace, shown) = {
            let chat = chat.read(cx);
            (chat.feed.snapshot(), chat.workspace.id.clone(), chat.session().cloned())
        };
        let sessions = records.sessions(&workspace);
        for session in sessions.iter().filter(|session| session.is_open()) {
            let (this, id) = (this.clone(), session.id.clone());
            let row = look::menu_row(session.label().to_string()).chosen(shown.as_ref() == Some(&session.id));
            let row = match why_it_needs_a_look(session) {
                Some(why) if shown.as_ref() != Some(&session.id) => row.note(why, NoteKind::Count),
                Some(_) | None => row,
            };
            menu = menu.row(row.on_choose(move |_, cx| {
                let _ = this.update(cx, |chat, cx| chat.open_chat(&id, cx));
            }));
        }
        let new = this.clone();
        let row = look::menu_row("New chat").icon(IconName::MessageSquarePlus);
        menu = menu.separator().row(row.on_choose(move |_, cx| {
            let _ = new.update(cx, ChatView::new_chat);
        }));
        let mut closed: Vec<(&Session, i64)> =
            sessions.iter().filter_map(|session| Some((session, session.closed_at_ms?))).collect();
        if closed.is_empty() {
            return menu;
        }
        closed.sort_by_key(|(_, closed_at_ms)| Reverse(*closed_at_ms));
        menu = menu.separator().label("Closed chats");
        let now_ms = look::now_ms(cx);
        for (session, closed_at_ms) in closed {
            let (this, id) = (this.clone(), session.id.clone());
            let ago = look::ago(closed_at_ms, now_ms);
            let row = look::menu_row(session.label().to_string()).note(ago, NoteKind::Detail);
            menu = menu.row(row.on_choose(move |_, cx| {
                let _ = this.update(cx, |chat, cx| chat.reopen(id.clone(), cx));
            }));
        }
        menu
    }
}

/// Why another of the workspace's chats needs a look, which its row says where the desktop's tab stands out: it's
/// waiting on an answer or an approval, or has a reply nobody has read.
fn why_it_needs_a_look(session: &Session) -> Option<&'static str> {
    if client_core::marking::waiting_on_you([session]) {
        Some("Waiting")
    } else if session.unread() {
        Some("Unread")
    } else {
        None
    }
}

impl ChatView {
    /// Shows another of the workspace's open chats in this one's place, with what's typed in it, as the hub keeps it.
    fn open_chat(&mut self, id: &SessionId, cx: &mut Context<Self>) {
        if let Some(session) = self.feed.snapshot().session(id).cloned() {
            self.switch_to(session, cx);
        }
    }

    /// Shows `session` in place of the chat shown, letting go of what that one followed, as its transcript goes, and
    /// what it opened over it.
    fn switch_to(&mut self, session: Session, cx: &mut Context<Self>) {
        if self.session() == Some(&session.id) {
            return;
        }
        self.over = Over::default();
        self.error = None;
        self.show(session, cx);
    }

    /// Starts another chat in the workspace, on the shown chat's agent, with its model and effort, as the desktop's
    /// New chat does, and shows it.
    fn new_chat(&mut self, cx: &mut Context<Self>) {
        let shown = self.shown.as_ref().map(|shown| &shown.session);
        let agent = shown.map_or_else(|| self.agents.for_new_chat(), |session| session.agent);
        let (id, settings) = (self.workspace.id.clone(), client_core::new_chat_settings(shown, agent));
        let client_request_id = Some(client_core::new_request_id());
        let made = self.hub.ask(SessionCreate { workspace_id: id, agent, settings, client_request_id });
        self.opening = Some(cx.spawn(async move |this, cx| {
            let made = made.recv().await;
            let _ = this.update(cx, |chat, cx| match made {
                Ok(session) => chat.switch_to(session, cx),
                Err(e) => chat.say(&couldnt("start a chat", &e), cx),
            });
        }));
    }

    /// Opens a closed chat again, and shows it.
    fn reopen(&mut self, id: SessionId, cx: &mut Context<Self>) {
        let reopened = self.hub.ask(SessionReopen { id });
        self.opening = Some(cx.spawn(async move |this, cx| {
            let reopened = reopened.recv().await;
            let _ = this.update(cx, |chat, cx| match reopened {
                Ok(session) => chat.switch_to(session, cx),
                Err(e) => chat.say(&couldnt("reopen the chat", &e), cx),
            });
        }));
    }
}
