//! A server's form, for one to add and one to edit: its name, its URL and how it knows the person, by signing in to it,
//! with a key or with neither. A key is handed to the hub and kept nowhere else: not on disk, not in a log and not in
//! the app's settings. Its field is made anew as it's sent, so nothing of it stays.

use client_core::new_tool_server_id;
use gpui_kit::component::input::InputEvent;
use gpui_kit::{App, Context, Subscription, Task, Window};
use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::{ToolServer, ToolServerAuth, ToolServerAuthGiven, ToolServerId, ToolServerKey};

use super::super::field::Field;
use super::edits::{self, Edits};
use super::kept::{Kept, Shows};
use super::words;
use crate::tool_sign_in::{EditedServer, NewServer, SignInTo};

/// The panel a server is added or edited in.
pub(super) struct Form {
    pub(super) of: Of,
    pub(super) url: Field,
    pub(super) name: Field,
    pub(super) auth: AuthFields,
    /// Why it wasn't added or saved when it was last tried, for the person to read.
    pub(super) refused: Option<String>,
    /// The wait for what the hub answers, while it hasn't. It goes with the panel, so an answer to a panel that was
    /// closed, or replaced by another, lands on none.
    asking: Option<Task<()>>,
    _url_typed: Subscription,
}

/// The server a form is of.
#[derive(Clone)]
pub(super) enum Of {
    /// One to add, whose ID is chosen as the panel opens, so adding it again, as when the hub's answer was lost, is the
    /// same server.
    New(ToolServerId),
    /// One the hub has, as it was when the panel opened, which what's typed is held against.
    Had(Box<ToolServer>),
}

/// What a form's main button does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Does {
    Add,
    Save,
    /// Signs in, and adds or saves the server once that's approved.
    SignIn,
}

/// How the server is to know the person, as it was chosen, with the fields that takes.
pub(super) enum AuthFields {
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

    /// The one `auth` is, or `None` for one only a newer hub has.
    pub(super) fn of(auth: &ToolServerAuth) -> Option<Way> {
        match auth {
            ToolServerAuth::SignIn { .. } => Some(Way::SignIn),
            ToolServerAuth::Key { .. } => Some(Way::Key),
            ToolServerAuth::Neither => Some(Way::Neither),
            ToolServerAuth::Unknown => None,
        }
    }
}

impl AuthFields {
    pub(super) fn way(&self) -> Way {
        match self {
            AuthFields::SignIn => Way::SignIn,
            AuthFields::Key { .. } => Way::Key,
            AuthFields::Neither => Way::Neither,
        }
    }
}

impl Form {
    /// The server as it's typed, held against what it was, for one the hub has.
    pub(super) fn edits(&self, cx: &App) -> Option<Edits> {
        let Of::Had(was) = &self.of else { return None };
        Some(Edits::of(&self.edited(was, cx), &self.auth, cx))
    }

    fn edited(&self, was: &ToolServer, cx: &App) -> EditedServer {
        EditedServer { was: was.clone(), name: self.name.value(cx), url: self.url.value(cx) }
    }

    pub(super) fn does(&self, cx: &App) -> Does {
        match self.edits(cx) {
            None if self.auth.way() == Way::SignIn => Does::SignIn,
            None => Does::Add,
            Some(edits) if edits.signs_in() => Does::SignIn,
            Some(_) => Does::Save,
        }
    }

    /// Whether it has what adding or saving the server takes.
    fn filled(&self, cx: &App) -> bool {
        let auth = match &self.auth {
            AuthFields::Key { header, key } => {
                header.filled(cx) && (key.filled(cx) || self.edits(cx).is_some_and(Edits::keeps_a_key))
            }
            AuthFields::SignIn | AuthFields::Neither => true,
        };
        self.url.filled(cx) && self.name.filled(cx) && auth
    }

    /// What its sign-in is to.
    fn signs_in_to(&self, cx: &App) -> SignInTo {
        match &self.of {
            Of::New(id) => {
                SignInTo::New(NewServer { id: id.clone(), name: self.name.value(cx), url: self.url.value(cx) })
            }
            Of::Had(was) => SignInTo::Edited(self.edited(was, cx)),
        }
    }

    /// What the hub is asked to save of `was`, which is only what's changed.
    fn edit(&self, was: &ToolServer, cx: &App) -> ToolServerEdit {
        let edited = self.edited(was, cx);
        let auth = edits::auth(&edited, &self.auth, cx);
        ToolServerEdit { id: was.id.clone(), name: edited.new_name(), url: edited.new_url(), auth }
    }

    /// How a new server is to know the person, as it's given to the hub. `None` for a sign-in, which is the person's to
    /// approve first.
    fn given(&self, cx: &App) -> Option<ToolServerAuthGiven> {
        match &self.auth {
            AuthFields::SignIn => None,
            AuthFields::Key { header, key } => {
                let key = ToolServerKey::new(key.value(cx));
                Some(ToolServerAuthGiven::Key { header: header.value(cx), key })
            }
            AuthFields::Neither => Some(ToolServerAuthGiven::Neither),
        }
    }
}

impl Kept {
    /// Opens the panel for adding a server, with the cursor in its name.
    pub(super) fn open_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let auth = AuthFields::SignIn;
        self.open_form(Of::New(new_tool_server_id()), [String::new(), String::new()], auth, None, window, cx);
    }

    /// Opens the panel for editing `server`, as it is, under its row, with the cursor in its name.
    pub(super) fn open_edit(&mut self, server: &ToolServer, window: &mut Window, cx: &mut Context<Self>) {
        let auth = match &server.auth {
            ToolServerAuth::SignIn { .. } => AuthFields::SignIn,
            ToolServerAuth::Key { header } => key_fields(header.as_str(), true, window, cx),
            ToolServerAuth::Neither => AuthFields::Neither,
            // It has no Edit.
            ToolServerAuth::Unknown => return,
        };
        let typed = [server.name.to_string(), server.url.as_str().to_owned()];
        self.open_form(Of::Had(Box::new(server.clone())), typed, auth, None, window, cx);
    }

    /// Opens the panel a sign-in to `to` was started from, with what was typed in it, and `why` that came to nothing.
    /// One to sign in to again was started from none.
    pub(super) fn open_sign_in_refused(
        &mut self,
        to: SignInTo,
        why: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (of, typed) = match to {
            SignInTo::New(NewServer { id, name, url }) => (Of::New(id), [name, url]),
            SignInTo::Edited(EditedServer { was, name, url }) => (Of::Had(Box::new(was)), [name, url]),
            SignInTo::Again { .. } => return,
        };
        self.open_form(of, typed, AuthFields::SignIn, Some(why), window, cx);
    }

    fn open_form(
        &mut self,
        of: Of,
        [name, url]: [String; 2],
        auth: AuthFields,
        refused: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url = Field::new(window, cx, |input| input.default_value(url), Kept::go);
        // The last made, so it has the cursor.
        let name = Field::new(window, cx, |input| input.default_value(name), Kept::go);
        let url_typed = cx.subscribe_in(&url.input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.url_typed(window, cx);
            }
        });
        let form = Form { of, url, name, auth, refused, asking: None, _url_typed: url_typed };
        self.open(Shows::Form(Box::new(form)), cx);
    }

    /// Has the server know the person by `way`, with the fields that takes, and the cursor in the key where it's a key.
    /// The name and the URL stay, and what was typed for another way goes.
    pub(super) fn choose(&mut self, way: Way, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form().filter(|form| form.auth.way() != way) else { return };
        // As it was, for the way the server already knows the person.
        let had = match &form.of {
            Of::Had(was) => match &was.auth {
                ToolServerAuth::Key { header } => Some(header.as_str().to_owned()),
                ToolServerAuth::SignIn { .. } | ToolServerAuth::Neither | ToolServerAuth::Unknown => None,
            },
            Of::New(_) => None,
        };
        let auth = match way {
            Way::SignIn => AuthFields::SignIn,
            Way::Key => key_fields(had.as_deref().unwrap_or(words::HEADER), had.is_some(), window, cx),
            Way::Neither => AuthFields::Neither,
        };
        if let Some(form) = self.form_mut() {
            (form.auth, form.refused) = (auth, None);
        }
        self.url_typed(window, cx);
        cx.notify();
    }

    /// Says in the key's field whether there's one to keep, which there isn't for another server.
    fn url_typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form() else { return };
        let AuthFields::Key { key, .. } = &form.auth else { return };
        let keeps = form.edits(cx).is_some_and(Edits::keeps_a_key);
        let says = if keeps { words::KEPT } else { "" };
        key.input.clone().update(cx, |input, cx| input.set_placeholder(says, window, cx));
    }

    fn form(&self) -> Option<&Form> {
        let Some(Shows::Form(form)) = self.panel.as_ref().map(|panel| &panel.shows) else { return None };
        Some(form)
    }

    fn form_mut(&mut self) -> Option<&mut Form> {
        let Some(Shows::Form(form)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(form)
    }

    /// Whether the form has what its button takes, and isn't waiting on the hub, with a hub to ask.
    pub(super) fn can_go(&self, cx: &App) -> bool {
        let Some(form) = self.form() else { return false };
        self.can_ask() && form.asking.is_none() && form.filled(cx)
    }

    /// Has the hub add the server the form says, or save what's changed of it, or signs in to it, which does either
    /// once it's approved.
    pub(super) fn go(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_go(cx) {
            return;
        }
        let (Some(hub), Some(form)) = (self.asking.clone(), self.form()) else { return };
        let does = form.does(cx);
        let answer = match (&form.of, does) {
            (_, Does::SignIn) => {
                let to = form.signs_in_to(cx);
                return self.sign_in(to, window, cx);
            }
            (Of::New(id), Does::Add | Does::Save) => {
                let Some(auth) = form.given(cx) else { return };
                let (id, name, url) = (id.clone(), form.name.value(cx), form.url.value(cx));
                hub.client.start(move |client| client.tool_server_add(&id, &name, &url, auth))
            }
            (Of::Had(was), Does::Add | Does::Save) => {
                let edit = form.edit(was, cx);
                hub.client.start(move |client| client.tool_server_edit(edit))
            }
        };
        let asking = cx.spawn_in(window, async move |this, cx| {
            let answer = answer.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.answered(does, answer, window, cx));
        });
        if let Some(form) = self.form_mut() {
            (form.refused, form.asking) = (None, Some(asking));
        }
        self.empty_the_key(window, cx);
        cx.notify();
    }

    /// Makes the key's field anew, so nothing of what's been sent of it stays.
    fn empty_the_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Form { auth: AuthFields::Key { key, .. }, .. }) = self.form_mut() {
            *key = Field::token(window, cx, Kept::go);
        }
        self.url_typed(window, cx);
    }

    /// Closes the panel once the hub has done what it was asked, and otherwise says why it couldn't. The panel is the
    /// one that asked, since the wait goes with it.
    fn answered(
        &mut self,
        does: Does,
        answer: client_core::Result<ToolServer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match answer {
            Ok(_) => self.put_away_panel(window, cx),
            Err(e) => {
                let Some(form) = self.form_mut() else { return };
                (form.refused, form.asking) = (Some(words::couldnt(words::to_do(does), &e)), None);
                cx.notify();
            }
        }
    }
}

/// The fields of a server a key is sent to in `header`, and which has one already while `kept`.
fn key_fields(header: &str, kept: bool, window: &mut Window, cx: &mut Context<Kept>) -> AuthFields {
    let header = header.to_owned();
    let header = Field::new(window, cx, |input| input.default_value(header), Kept::go);
    let key = Field::token(window, cx, Kept::go);
    if kept {
        key.input.update(cx, |input, cx| input.set_placeholder(words::KEPT, window, cx));
    }
    AuthFields::Key { header, key }
}
