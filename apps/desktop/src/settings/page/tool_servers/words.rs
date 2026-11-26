//! What MCP servers says in words: what the list says of itself, what a panel's fields take, what removing a server
//! asks, and what the hub not doing what was asked means, with why it refused as client-core's `refusals` says it.

use client_core::{ClientError, refusals};
use gpui_kit::SharedString;
use protocol::model::{ToolServerAuth, ToolServerName};

/// What a search finds the page by, besides its name.
pub(super) const ABOUT: &str = "The tools every agent has from servers you add, with a key or without.";

/// What Added says of itself under its heading.
pub(super) const ADDED_SAYS: &str = "Every agent has these, whichever machine it's on, beside the ones its own CLI has. \
                                     This hub keeps their keys.";

/// What Added says while the hub has no server of its own.
pub(super) const NONE_ADDED: &str = "None yet. Add one, and every agent has it.";

/// What's said in place of a row's line when the page was to change something with no hub to ask.
pub(super) const NO_HUB: &str = "There's no hub to ask.";

/// What a server that doesn't know the person is for.
pub(super) const NEITHER: &str = "For a server that anyone who can reach it can use.";

/// The header a key is sent in, unless it's changed.
pub(super) const HEADER: &str = "Authorization";

/// What goes in a key's field.
pub(super) const KEY_SAYS: &str = "What the server gave you. Only the hub keeps it.";

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

/// What a row says of how its server knows the person, where that isn't plain.
pub(super) fn known_by(auth: &ToolServerAuth) -> Option<&'static str> {
    match auth {
        ToolServerAuth::Key { .. } => Some("Key"),
        // `Unknown` is a way only a newer hub has, which this app has no word for.
        ToolServerAuth::Neither | ToolServerAuth::Unknown => None,
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
