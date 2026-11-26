//! A server's form, for one to add and one to edit: its name, its URL and how it knows the person, by signing in to it,
//! with a key or with neither. A key is handed to the hub and kept nowhere else: not on disk, not in a log and not in
//! the app's settings. It's in its field until the server is added or saved, or the panel is closed, and goes with
//! that.

use client_core::new_tool_server_id;
use client_core::tool_server_edits::{Does, Edit, Sent, Typed, TypedAuth, Way};
use gpui_kit::{App, Context, Window};
use protocol::client::tool_server_checks::ToolServerToCheck;
use protocol::client::tool_servers::ToolServerEdit;
use protocol::model::{ToolServer, ToolServerAuth, ToolServerId, ToolServerKey};

use super::super::field::Field;
use super::going::FormStands;
use super::kept::{Kept, Shows};
use super::words;
use crate::tool_sign_in::{EditedServer, NewServer, SignInTo};

/// The panel a server is added or edited in.
pub(super) struct Form {
    pub(super) of: Of,
    pub(super) url: Field,
    pub(super) name: Field,
    pub(super) auth: AuthFields,
    pub(super) stands: FormStands,
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

impl AuthFields {
    pub(super) fn way(&self) -> Way {
        match self {
            AuthFields::SignIn => Way::SignIn,
            AuthFields::Key { .. } => Way::Key,
            AuthFields::Neither => Way::Neither,
        }
    }

    fn typed(&self, cx: &App) -> TypedAuth {
        match self {
            AuthFields::SignIn => TypedAuth::SignIn,
            AuthFields::Key { header, key } => {
                TypedAuth::Key { header: header.value(cx), key: ToolServerKey::new(key.value(cx)) }
            }
            AuthFields::Neither => TypedAuth::Neither,
        }
    }
}

impl Form {
    /// What's typed in it, for client-core's `tool_server_edits` to say what comes of it.
    fn typed(&self, cx: &App) -> Typed {
        Typed { name: self.name.value(cx), url: self.url.value(cx), auth: self.auth.typed(cx) }
    }

    /// The server the hub has, for a form that edits one.
    fn had(&self) -> Option<&ToolServer> {
        match &self.of {
            Of::Had(was) => Some(was),
            Of::New(_) => None,
        }
    }

    /// What's typed, as an edit of the server the hub has, for a form that edits one.
    pub(super) fn edit(&self, cx: &App) -> Option<Edit> {
        self.had().map(|was| self.typed(cx).edit_of(was))
    }

    fn edited(&self, was: &ToolServer, cx: &App) -> EditedServer {
        EditedServer { was: was.clone(), name: self.name.value(cx), url: self.url.value(cx) }
    }

    pub(super) fn does(&self, cx: &App) -> Does {
        self.typed(cx).does(self.had())
    }

    /// Whether it has what adding or saving the server takes.
    pub(super) fn filled(&self, cx: &App) -> bool {
        self.typed(cx).is_complete(self.had())
    }

    /// What its sign-in is to.
    pub(super) fn signs_in_to(&self, cx: &App) -> SignInTo {
        match &self.of {
            Of::New(id) => {
                SignInTo::New(NewServer { id: id.clone(), name: self.name.value(cx), url: self.url.value(cx) })
            }
            Of::Had(was) => SignInTo::Edited(self.edited(was, cx)),
        }
    }

    /// What the hub is asked to add, or to save of what it has, which is only what's changed. `None` for a server to
    /// add by signing in to it, which is the person's to approve first.
    pub(super) fn asks_for(&self, cx: &App) -> Option<ToolServerToCheck> {
        let typed = self.typed(cx);
        Some(match &self.of {
            Of::New(id) => ToolServerToCheck::Add(typed.added(id)?),
            Of::Had(was) => {
                let Sent { name, url, auth } = typed.edit_of(was).into_sent();
                ToolServerToCheck::Edit(ToolServerEdit { id: was.id.clone(), name, url, auth, saved_anyway: false })
            }
        })
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
        let url = Field::new(window, cx, |input| input.default_value(url), Kept::go).on_change(window, cx, Kept::typed);
        // The last made, so it has the cursor.
        let name =
            Field::new(window, cx, |input| input.default_value(name), Kept::go).on_change(window, cx, Kept::typed);
        let stands = refused.map_or(FormStands::Open, FormStands::Refused);
        let form = Form { of, url, name, auth, stands };
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
            form.auth = auth;
            form.stands.way_chosen();
        }
        self.say_whether_kept(window, cx);
        cx.notify();
    }

    /// Something was typed in a field, so what was checked is no longer what's there.
    fn typed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(form) = self.form_mut() {
            form.stands.field_changed();
        }
        self.say_whether_kept(window, cx);
        cx.notify();
    }

    /// Says in the key's field whether there's one to keep, which there isn't for another server.
    fn say_whether_kept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.form() else { return };
        let AuthFields::Key { key, .. } = &form.auth else { return };
        let keeps = form.edit(cx).is_some_and(|edit| edit.keeps_a_key());
        let says = if keeps { words::KEPT } else { "" };
        key.input.clone().update(cx, |input, cx| input.set_placeholder(says, window, cx));
    }

    pub(super) fn form(&self) -> Option<&Form> {
        let Some(Shows::Form(form)) = self.panel.as_ref().map(|panel| &panel.shows) else { return None };
        Some(form)
    }

    pub(super) fn form_mut(&mut self) -> Option<&mut Form> {
        let Some(Shows::Form(form)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(form)
    }
}

/// The fields of a server a key is sent to in `header`, and which has one already while `kept`.
fn key_fields(header: &str, kept: bool, window: &mut Window, cx: &mut Context<Kept>) -> AuthFields {
    let header = header.to_owned();
    let header =
        Field::new(window, cx, |input| input.default_value(header), Kept::go).on_change(window, cx, Kept::typed);
    let key = Field::token(window, cx, Kept::go).on_change(window, cx, Kept::typed);
    if kept {
        key.input.update(cx, |input, cx| input.set_placeholder(words::KEPT, window, cx));
    }
    AuthFields::Key { header, key }
}
