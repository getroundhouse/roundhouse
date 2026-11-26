//! Adding a server, in one panel, by signing in to it, with a key or with neither. A key is handed to the hub and kept
//! nowhere else: not on disk, not in a log and not in the app's settings. Its field is made anew as it's sent, so nothing
//! of it stays.

use client_core::new_tool_server_id;
use gpui_kit::{App, Context, Task, Window};
use protocol::model::{ToolServer, ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::super::field::Field;
use super::kept::{Kept, Shows};
use super::words;
use crate::tool_sign_in::{NewServer, SignInTo};

/// The panel for adding a server.
pub(super) struct AddPanel {
    /// The server's, chosen as the panel opens, so adding it again, as when the hub's answer was lost, is the same
    /// server.
    id: ToolServerId,
    pub(super) url: Field,
    pub(super) name: Field,
    pub(super) auth: AuthFields,
    /// Why it wasn't added when it was last tried, for the person to read.
    pub(super) refused: Option<String>,
    /// The wait for what the hub answers, while it hasn't. It goes with the panel, so an answer to a panel that was
    /// closed, or replaced by another, lands on none.
    adding: Option<Task<()>>,
}

/// How the server is to know the person, as it was chosen, with the fields that takes.
pub(super) enum AuthFields {
    /// By a sign-in, which adds it once it's approved.
    SignIn,
    /// By a key, whose field shows it masked.
    Key {
        header: Field,
        key: Field,
    },
    Neither,
}

/// A way a server can know the person, as the panel's dropdown has them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Way {
    SignIn,
    Key,
    Neither,
}

impl Way {
    /// Every way, in the dropdown's order.
    pub(super) const ALL: [Way; 3] = [Way::SignIn, Way::Key, Way::Neither];
}

impl AuthFields {
    pub(super) fn way(&self) -> Way {
        match self {
            AuthFields::SignIn => Way::SignIn,
            AuthFields::Key { .. } => Way::Key,
            AuthFields::Neither => Way::Neither,
        }
    }

    fn filled(&self, cx: &App) -> bool {
        match self {
            AuthFields::Key { header, key } => header.filled(cx) && key.filled(cx),
            AuthFields::SignIn | AuthFields::Neither => true,
        }
    }

    /// As it's given to the hub, with the key out of its field, which is made anew, or `None` for a sign-in, which is
    /// the person's to approve first.
    fn take(&mut self, window: &mut Window, cx: &mut Context<Kept>) -> Option<ToolServerAuthGiven> {
        match self {
            AuthFields::SignIn => None,
            AuthFields::Key { header, key } => {
                let given = ToolServerKey::new(key.value(cx));
                *key = Field::token(window, cx, Kept::add);
                Some(ToolServerAuthGiven::Key { header: header.value(cx), key: given })
            }
            AuthFields::Neither => Some(ToolServerAuthGiven::Neither),
        }
    }
}

impl Kept {
    /// Opens the panel for adding a server, with the cursor in its name.
    pub(super) fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let nothing_typed = NewServer { id: new_tool_server_id(), name: String::new(), url: String::new() };
        self.open_adding(nothing_typed, None, window, cx);
    }

    /// Opens the panel for adding a server as its sign-in was started from, with what was `typed` of it, and `why` that
    /// came to nothing.
    pub(super) fn open_sign_in_refused(
        &mut self,
        typed: NewServer,
        why: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_adding(typed, Some(why), window, cx);
    }

    fn open_adding(&mut self, typed: NewServer, refused: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let NewServer { id, name, url } = typed;
        let url = Field::new(window, cx, |input| input.default_value(url), Kept::add);
        // The last made, so it has the cursor.
        let name = Field::new(window, cx, |input| input.default_value(name), Kept::add);
        let panel = AddPanel { id, url, name, auth: AuthFields::SignIn, refused, adding: None };
        self.open(Shows::Adding(Box::new(panel)), cx);
    }

    /// Has the server know the person by `way`, with the fields that takes, and the cursor in the key where it's a key.
    /// The name and the URL stay, and what was typed for another way goes.
    pub(super) fn choose(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        if self.adding().is_none_or(|panel| panel.auth.way() == way) {
            return;
        }
        let auth = match way {
            Way::SignIn => AuthFields::SignIn,
            Way::Key => key_fields(window, cx),
            Way::Neither => AuthFields::Neither,
        };
        if let Some(panel) = self.adding_mut() {
            (panel.auth, panel.refused) = (auth, None);
        }
        cx.notify();
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

    /// Has the hub add the server the panel says, or signs in to it, which adds it once that's approved.
    pub(super) fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_add(cx) {
            return;
        }
        let (Some(asking), Some(panel)) = (self.asking.clone(), self.adding_mut()) else { return };
        let (id, name, url) = (panel.id.clone(), panel.name.value(cx), panel.url.value(cx));
        let Some(auth) = panel.auth.take(window, cx) else {
            return self.sign_in(SignInTo::New(NewServer { id, name, url }), window, cx);
        };
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

/// The fields of a server a key is sent to.
fn key_fields(window: &mut Window, cx: &mut Context<Kept>) -> AuthFields {
    let header = Field::new(window, cx, |input| input.default_value(words::HEADER), Kept::add);
    AuthFields::Key { header, key: Field::token(window, cx, Kept::add) }
}
