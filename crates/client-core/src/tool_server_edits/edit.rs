use protocol::model::{ToolServer, ToolServerAuth, ToolServerAuthEdit, ToolServerUrl, ToolSignInStatus};

use super::typed::key_typed;
use super::{Typed, TypedAuth, Way};

/// An edit of a server the hub has, as it's typed so far: what it changes, what saving it takes for the server to know
/// the person, and what the hub is sent to save it.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub(super) url: Url,
    pub(super) takes: Takes,
    sent: Sent,
}

/// What the hub is sent to save an edit, which is only what's changed, so what another client has changed meanwhile
/// stays. Each is as it was typed.
#[derive(Clone, Debug, PartialEq)]
pub struct Sent {
    pub name: Option<String>,
    pub url: Option<String>,
    /// `None` where that hasn't changed, and for a sign-in, which is the person's to approve first.
    pub auth: Option<ToolServerAuthEdit>,
}

/// What's typed of a server's URL, held against the one it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Url {
    AsItWas,
    /// The same server's, as another query, by which it may have other tools.
    AnotherQuery,
    /// Another server's.
    Moved,
    /// Nothing that's a URL yet, as while one is typed. It's taken as the one the server has for what the edit changes
    /// and takes, since the hub refuses it and so it changes nothing, and so what's said of the rest doesn't change with
    /// each key. It's still sent, for the hub to say why it's refused.
    Invalid,
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

impl Edit {
    /// Of `was`, the server the hub has, to what's `typed`.
    pub(super) fn of(was: &ToolServer, typed: &Typed) -> Edit {
        let url = Url::of(&typed.url, &was.url);
        let changed = |typed: &str, was: &str| (typed.trim() != was).then(|| typed.to_owned());
        let sent = Sent {
            name: changed(&typed.name, was.name.as_str()),
            url: changed(&typed.url, was.url.as_str()),
            auth: auth(was, typed),
        };
        Edit { url, takes: takes(was, typed.auth.way(), url), sent }
    }

    /// Whether saving it takes signing in to the server.
    pub fn signs_in(&self) -> bool {
        self.takes == Takes::SignIn
    }

    /// Whether the server's key is kept where none is typed.
    pub fn keeps_a_key(&self) -> bool {
        self.takes == Takes::KeyOrTheKept
    }

    pub fn into_sent(self) -> Sent {
        self.sent
    }

    pub(super) fn changes_name(&self) -> bool {
        self.sent.name.is_some()
    }

    /// Whether it changes how the server knows the person, by signing in or as the hub is sent it.
    pub(super) fn changes_auth(&self) -> bool {
        self.signs_in() || self.sent.auth.is_some()
    }
}

impl Url {
    fn of(typed: &str, was: &ToolServerUrl) -> Url {
        if typed.trim() == was.as_str() {
            return Url::AsItWas;
        }
        match ToolServerUrl::typed(typed) {
            Err(_) => Url::Invalid,
            Ok(url) if url.is_same_server(was) => Url::AnotherQuery,
            Ok(_) => Url::Moved,
        }
    }
}

fn takes(was: &ToolServer, way: Way, url: Url) -> Takes {
    let same = Way::of(&was.auth) == Some(way) && url != Url::Moved;
    match way {
        Way::SignIn if !same => Takes::SignIn,
        Way::SignIn if is_signed_in(&was.auth) => Takes::StayingSignedIn,
        Way::Key if same => Takes::KeyOrTheKept,
        Way::SignIn | Way::Key | Way::Neither => Takes::WhatsTyped,
    }
}

fn is_signed_in(auth: &ToolServerAuth) -> bool {
    matches!(auth, ToolServerAuth::SignIn { status: ToolSignInStatus::Works | ToolSignInStatus::NeedsMoreAccess })
}

/// How `was` is to know the person from now on, as the hub is given it. `None` where that hasn't changed, and for a
/// sign-in, which is the person's to approve first.
fn auth(was: &ToolServer, typed: &Typed) -> Option<ToolServerAuthEdit> {
    match &typed.auth {
        TypedAuth::SignIn => None,
        TypedAuth::Neither if was.auth == ToolServerAuth::Neither => None,
        TypedAuth::Neither => Some(ToolServerAuthEdit::Neither),
        TypedAuth::Key { header, key } => {
            let key = key_typed(key).cloned();
            let as_it_was = matches!(&was.auth, ToolServerAuth::Key { header: was } if was.as_str() == header);
            if as_it_was && key.is_none() {
                return None;
            }
            Some(ToolServerAuthEdit::Key { header: header.clone(), key, url: typed.url.clone() })
        }
    }
}
