//! What MCP servers says in words: what the list says of itself, what a panel's fields take, what removing a server
//! asks, and what the hub not doing what was asked means, with why it refused as client-core's `refusals` says it.

use client_core::tool_sign_ins::NoPage;
use client_core::{ClientError, refusals};
use gpui_kit::SharedString;
use protocol::model::{ToolServerAuth, ToolServerName, ToolSignInStatus};

use super::super::account_row::Tone;
use super::adding::Way;
use crate::names::MACHINE;

/// What a search finds the page by, besides its name.
pub(super) const ABOUT: &str =
    "The tools every agent has from servers you add, signed in to, with a key or with neither.";

/// What Added says of itself under its heading.
pub(super) const ADDED_SAYS: &str = "Every agent has these, whichever machine it's on, beside the ones its own CLI has. \
                                     This hub keeps their sign-ins.";

/// What Added says while the hub has no server of its own.
pub(super) const NONE_ADDED: &str = "None yet. Add one, and sign in to it once for every agent.";

/// What's said in place of a row's line when the page was to change something with no hub to ask.
pub(super) const NO_HUB: &str = "There's no hub to ask.";

/// What adding a server does, under its panel's title.
pub(super) const ADD: &str = "Every agent has its tools once it's added.";

/// What signing in to a server does.
pub(super) const SIGN_IN: &str = "Your browser opens on its own page. Every agent has its tools once you've signed in.";

/// What a sign-in says while the person has its page.
pub(super) const WAITING: &str = "Waiting for you to approve it in your browser.";

/// What it says while the hub is getting what the person approved.
pub(super) const FINISHING: &str = "Finishing the sign-in.";

/// The header a key is sent in, unless it's changed.
pub(super) const HEADER: &str = "Authorization";

/// What goes in its field.
pub(super) const HEADER_SAYS: &str = "Most servers want this one.";

/// What goes in a key's field.
pub(super) const KEY_SAYS: &str = "What the server gave you.";

/// What `way` is called in the panel's dropdown.
pub(super) fn way_called(way: Way) -> &'static str {
    match way {
        Way::SignIn => "Sign in",
        Way::Key => "Key",
        Way::Neither => "None",
    }
}

/// What the dropdown's menu says beside `way`.
pub(super) fn way_note(way: Way) -> &'static str {
    match way {
        Way::SignIn => "In your browser",
        Way::Key => "Sent with every request",
        Way::Neither => "Anyone can use it",
    }
}

/// What's said under the dropdown while `way` is chosen.
pub(super) fn way_says(way: Way) -> &'static str {
    match way {
        Way::SignIn => "Your browser opens on its own page, and the hub renews the sign-in by itself.",
        Way::Key => "Sent in a header with every request, such as an API key. Only the hub keeps it.",
        Way::Neither => "For a server that anyone who can reach it can use.",
    }
}

/// What the panel's main button does, by the way that's chosen.
pub(super) fn add_does(way: Way) -> &'static str {
    match way {
        Way::SignIn => "Sign in",
        Way::Key | Way::Neither => "Add",
    }
}

/// What's said under a URL's field, where `there` is the name of the server that's at what's typed, if one is.
pub(super) fn url_says(there: Option<&ToolServerName>) -> SharedString {
    match there {
        Some(there) => format!("You've already added this one, as {there}.").into(),
        None => "The server's own page has it.".into(),
    }
}

/// What's said under a name's field, as [`url_says`] says of a URL's.
pub(super) fn name_says(there: Option<&ToolServerName>) -> SharedString {
    match there {
        Some(there) => format!("So you and agents can tell it from {there}.").into(),
        None => "What you and agents call it.".into(),
    }
}

/// The title of the panel of a sign-in to the server called `name`.
pub(super) fn sign_in_title(name: &str) -> String {
    format!("Sign in to {name}")
}

/// What a sign-in says while the hub asks the server at `host` how it's signed in to.
pub(super) fn starting(host: &str) -> String {
    format!("Asking {host} how you sign in to it. This can take half a minute.")
}

/// What a sign-in says while the person has its page, which is on `host`.
pub(super) fn approve_on(host: &str) -> String {
    format!("Approve Roundhouse on {host}, in the browser on this {MACHINE}. Roundhouse never sees your password.")
}

/// Why a new server wasn't signed in to, where there's no page to sign in on, with how else it may be added.
pub(super) fn add_another_way(no_page: &NoPage) -> String {
    let add = match no_page {
        NoPage::NoSignIn => "To add it, choose None, or Key if it gave you one.",
        NoPage::CantSignIn(_) => "It may take a key.",
    };
    format!("{} {add}", no_page.why())
}

/// What a row says of how its server knows the person, where that isn't plain, and how it's said.
pub(super) fn known_by(auth: &ToolServerAuth) -> Option<(&'static str, Tone)> {
    match auth {
        ToolServerAuth::Key { .. } => Some(("Key", Tone::Plain)),
        ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsSignIn } => {
            Some(("Needs you to sign in", Tone::Problem))
        }
        // Not that it needs it, since it works for all else.
        ToolServerAuth::SignIn { status: ToolSignInStatus::NeedsMoreAccess } => {
            Some(("Sign in for more access", Tone::Problem))
        }
        // `Unknown` is a way, or a state of one, only a newer hub has, which this app has no word for.
        ToolServerAuth::SignIn { status: ToolSignInStatus::Works | ToolSignInStatus::Unknown }
        | ToolServerAuth::Neither
        | ToolServerAuth::Unknown => None,
    }
}

/// What removing the server called `name` asks first.
pub(super) fn remove_asks(name: &ToolServerName) -> String {
    format!("Remove {name}?")
}

/// What the question before removing a server says under it, by what the hub forgets with it.
pub(super) fn remove_says(auth: &ToolServerAuth) -> &'static str {
    match auth {
        ToolServerAuth::Key { .. } => {
            "Roundhouse forgets the server and its key, so using it again takes adding it again. The key isn't revoked \
             at the server, so it still works wherever else it's kept."
        }
        ToolServerAuth::SignIn { .. } => {
            "Roundhouse forgets the server and its sign-in, so using it again takes signing in to it again. The sign-in \
             isn't revoked at the server."
        }
        ToolServerAuth::Neither | ToolServerAuth::Unknown => {
            "Roundhouse forgets the server, so using it again takes adding it again."
        }
    }
}

/// What to say when the hub couldn't do `what` for a server, such as "remove it", or no longer has it.
pub(super) fn couldnt(what: &str, e: &ClientError) -> String {
    if e.is_not_found() {
        return refusals::gone("MCP server");
    }
    refusals::couldnt(what, e)
}
