//! What's been changed of a server the hub has, in its form: which of what it takes, and what that does.

use gpui_kit::App;
use protocol::model::{ToolServerAuth, ToolServerAuthEdit, ToolServerKey, ToolSignInStatus};

use super::form::{AuthFields, Way};
use crate::tool_sign_in::EditedServer;

/// What's been changed of a server, so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Edits {
    pub(super) name: bool,
    /// Its URL is another server's.
    pub(super) moved: bool,
    /// Anything else: its URL's query, or how it knows the person.
    pub(super) rest: bool,
    pub(super) takes: Takes,
}

/// What saving an edit takes for the server to know the person, by the way that's chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Takes {
    /// Nothing, since it's signed in to, and stays so.
    StayingSignedIn,
    SignIn,
    /// A key, or none, to keep the one it has.
    KeyOrTheKept,
    /// What's typed, which for a key is one, and otherwise is nothing.
    WhatsTyped,
}

impl Edits {
    /// Of `edited`, with how it's to know the person as `auth` has it.
    pub(super) fn of(edited: &EditedServer, auth: &AuthFields, cx: &App) -> Edits {
        let moved = edited.is_moved();
        let takes = takes(edited, auth.way(), moved);
        let auth_changed = takes == Takes::SignIn || self::auth(edited, auth, cx).is_some();
        let rest = edited.new_url().is_some() || auth_changed;
        Edits { name: edited.new_name().is_some(), moved, rest, takes }
    }

    pub(super) fn signs_in(self) -> bool {
        self.takes == Takes::SignIn
    }

    pub(super) fn keeps_a_key(self) -> bool {
        self.takes == Takes::KeyOrTheKept
    }
}

fn takes(edited: &EditedServer, way: Way, moved: bool) -> Takes {
    let same = Way::of(&edited.was.auth) == Some(way) && !moved;
    match way {
        Way::SignIn if !same => Takes::SignIn,
        Way::SignIn if edited.is_where_it_was() && is_signed_in(&edited.was.auth) => Takes::StayingSignedIn,
        Way::Key if same => Takes::KeyOrTheKept,
        Way::SignIn | Way::Key | Way::Neither => Takes::WhatsTyped,
    }
}

fn is_signed_in(auth: &ToolServerAuth) -> bool {
    matches!(auth, ToolServerAuth::SignIn { status: ToolSignInStatus::Works | ToolSignInStatus::NeedsMoreAccess })
}

/// How `edited` is to know the person from now on, as the hub is given it. `None` where that hasn't changed, and for a
/// sign-in, which is the person's to approve first.
pub(super) fn auth(edited: &EditedServer, auth: &AuthFields, cx: &App) -> Option<ToolServerAuthEdit> {
    match auth {
        AuthFields::SignIn => None,
        AuthFields::Neither if edited.was.auth == ToolServerAuth::Neither => None,
        AuthFields::Neither => Some(ToolServerAuthEdit::Neither),
        AuthFields::Key { header, key } => {
            let key = key.filled(cx).then(|| ToolServerKey::new(key.value(cx)));
            let header = header.value(cx);
            let as_it_was = matches!(&edited.was.auth, ToolServerAuth::Key { header: was } if was.as_str() == header);
            if as_it_was && key.is_none() {
                return None;
            }
            Some(ToolServerAuthEdit::Key { header, key, url: edited.url.clone() })
        }
    }
}
