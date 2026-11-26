//! Adding a server, with a key or with neither. A key is handed to the hub and kept nowhere else: not on disk, not in a
//! log and not in the app's settings. Its field is made anew as it's sent, so nothing of it stays.

use client_core::new_tool_server_id;
use gpui_kit::{App, Context, Task, Window};
use protocol::model::{ToolServer, ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::super::field::Field;
use super::kept::{Kept, Shows};
use super::words;

/// The panel for adding a server.
pub(super) struct AddPanel {
    /// The server's, chosen as the panel opens, so adding it again, as when the hub's answer was lost, is the same
    /// server.
    id: ToolServerId,
    pub(super) url: Field,
    pub(super) name: Field,
    pub(super) auth: AuthFields,
    /// What the hub said when it last refused, for the person to read.
    pub(super) refused: Option<String>,
    /// The wait for what the hub answers, while it hasn't. It goes with the panel, so an answer to a panel that was
    /// closed, or replaced by another, lands on none.
    adding: Option<Task<()>>,
}

/// How the server is to know the person, as it was chosen, with the fields that takes.
pub(super) enum AuthFields {
    /// By a key, whose field shows it masked.
    Key {
        header: Field,
        key: Field,
    },
    Neither,
}

impl AuthFields {
    fn filled(&self, cx: &App) -> bool {
        match self {
            AuthFields::Key { header, key } => header.filled(cx) && key.filled(cx),
            AuthFields::Neither => true,
        }
    }

    /// As it's given to the hub, with the key out of its field, which is made anew.
    fn take(&mut self, window: &mut Window, cx: &mut Context<Kept>) -> ToolServerAuthGiven {
        match self {
            AuthFields::Key { header, key } => {
                let given = ToolServerKey::new(key.value(cx));
                *key = Field::token(window, cx, Kept::add);
                ToolServerAuthGiven::Key { header: header.value(cx), key: given }
            }
            AuthFields::Neither => ToolServerAuthGiven::Neither,
        }
    }
}

impl Kept {
    /// Opens the panel for adding a server a key is sent to, with the cursor in its URL.
    pub(super) fn open_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let header = Field::new(window, cx, |input| input.default_value(words::HEADER), Kept::add);
        let key = Field::token(window, cx, Kept::add);
        self.open_adding(AuthFields::Key { header, key }, window, cx);
    }

    /// Opens the panel for adding a server that doesn't know the person, with the cursor in its URL.
    pub(super) fn open_neither(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_adding(AuthFields::Neither, window, cx);
    }

    fn open_adding(&mut self, auth: AuthFields, window: &mut Window, cx: &mut Context<Self>) {
        let name = Field::new(window, cx, |input| input, Kept::add);
        // The last made, so it has the cursor.
        let url = Field::new(window, cx, |input| input, Kept::add);
        let panel = AddPanel { id: new_tool_server_id(), url, name, auth, refused: None, adding: None };
        self.open(Shows::Adding(Box::new(panel)), cx);
    }

    fn adding(&self) -> Option<&AddPanel> {
        let Some(Shows::Adding(panel)) = self.panel.as_ref().map(|panel| &panel.shows) else { return None };
        Some(panel)
    }

    fn adding_mut(&mut self) -> Option<&mut AddPanel> {
        let Some(Shows::Adding(panel)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(panel)
    }

    /// Whether the panel has what adding a server takes, and isn't waiting on the hub, with a hub to ask.
    pub(super) fn can_add(&self, cx: &App) -> bool {
        let Some(panel) = self.adding() else { return false };
        let filled = panel.url.filled(cx) && panel.name.filled(cx) && panel.auth.filled(cx);
        self.can_ask() && panel.adding.is_none() && filled
    }

    /// Has the hub add the server the panel says.
    pub(super) fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_add(cx) {
            return;
        }
        let (Some(asking), Some(panel)) = (self.asking.clone(), self.adding_mut()) else { return };
        let (id, name, url) = (panel.id.clone(), panel.name.value(cx), panel.url.value(cx));
        let auth = panel.auth.take(window, cx);
        let adding = asking.client.start(move |client| client.tool_server_add(&id, &name, &url, auth));
        panel.refused = None;
        panel.adding = Some(cx.spawn_in(window, async move |this, cx| {
            let answer = adding.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.answered(answer, window, cx));
        }));
        cx.notify();
    }

    /// Closes the panel once the hub has added the server, and otherwise says why it couldn't. The panel is the one that
    /// asked, since the wait goes with it.
    fn answered(&mut self, answer: client_core::Result<ToolServer>, window: &mut Window, cx: &mut Context<Self>) {
        match answer {
            Ok(_) => self.put_away_panel(window, cx),
            Err(e) => {
                let Some(panel) = self.adding_mut() else { return };
                (panel.refused, panel.adding) = (Some(words::couldnt("add it", &e)), None);
                cx.notify();
            }
        }
    }
}
