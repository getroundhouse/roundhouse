//! What MCP servers says in words: what the list says of itself, what a panel's fields take, what removing a server
//! asks, and what the hub not doing what was asked means, with why it refused as client-core's `refusals` says it. What
//! an edit does, and what saving it takes, are client-core's `tool_server_edits` to say.

use client_core::tool_server_edits::{Does, SIGN_IN_TO_SAVE, Way};
use client_core::tool_sign_ins::{self, NoPage};
use client_core::{ClientError, refusals};
use gpui_kit::SharedString;
use protocol::model::{ToolServerAuth, ToolServerName, ToolServerUrl, ToolSignInStatus};

use super::super::account_row::Tone;
use super::super::field::Says;
use crate::names::MACHINE;
use crate::tool_sign_in::SignInTo;

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
const SIGN_IN: &str = "Your browser opens on its own page. Every agent has its tools once you've signed in.";

/// What a sign-in says while the person has its page.
pub(super) const WAITING: &str = "Waiting for you to approve it in your browser.";

/// What it says while the hub is getting what the person approved.
pub(super) const FINISHING: &str = "Finishing the sign-in.";

/// The header a key is sent in, unless it's changed.
pub(super) const HEADER: &str = "Authorization";

/// What goes in its field.
pub(super) const HEADER_SAYS: &str = "Most servers want this one.";

/// What a key's field says in place of a key, while the server has one to keep.
pub(super) const KEPT: &str = "Kept";

/// What's said under a URL that looks like it has a key in it.
const LOOKS_KEYED: &str = "This looks like it has a key in it. Every app on this hub can see the URL. If the server \
                           takes a key in a header, use Key.";

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

/// The title of the panel the server called `name` is edited in.
pub(super) fn edit_title(name: &ToolServerName) -> String {
    format!("Edit {name}")
}

/// What a form's main button says.
pub(super) fn does(does: Does) -> &'static str {
    match does {
        Does::Add => "Add",
        Does::Save => "Save",
        Does::SignIn => "Sign in",
    }
}

/// What the main button says of a form whose server didn't list its tools, and of a sign-in that failed.
pub(super) const TRY_AGAIN: &str = "Try again";

/// What the button says that does what a form's main button `does` though the server didn't list its tools.
pub(super) fn anyway(does: &str) -> String {
    format!("{does} anyway")
}

/// What the hub couldn't do, where it couldn't do what a form's button does.
pub(super) fn to_do(does: Does) -> &'static str {
    match does {
        Does::Add => "add it",
        Does::Save => "save it",
        Does::SignIn => "sign in",
    }
}

/// What's said under the field of a URL that's `typed`, where `there` is the name of the server that's there, if one
/// is.
pub(super) fn url_says(typed: &str, there: Option<&ToolServerName>) -> Says {
    if ToolServerUrl::looks_keyed(typed) {
        return Says::Warning(LOOKS_KEYED.into());
    }
    match there {
        Some(there) => Says::Plain(format!("You've already added this one, as {there}.").into()),
        None => Says::Plain("The server's own page has it.".into()),
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

/// What signing in to `to` does.
pub(super) fn sign_in(to: &SignInTo) -> &'static str {
    match to {
        SignInTo::New(_) | SignInTo::Again { .. } => SIGN_IN,
        SignInTo::Edited(_) => SIGN_IN_TO_SAVE,
    }
}

/// What a sign-in says while the hub asks the server at `host` how it's signed in to.
pub(super) fn starting(host: &str) -> String {
    format!("Asking {host} how you sign in to it. This can take half a minute.")
}

/// What a sign-in to `to` says while the person has its page, which is on the host `page`.
pub(super) fn approve_on(page: &str, to: &SignInTo) -> String {
    tool_sign_ins::approve_on(page, &to.host(), MACHINE)
}

/// Why a server wasn't signed in to, where there's no page to sign in on, with how else it may be done `to_do`, such as
/// "add".
pub(super) fn another_way(no_page: &NoPage, to_do: &str) -> String {
    let why = no_page.why();
    match no_page {
        NoPage::NoSignIn => format!("{why} To {to_do} it, choose None, or Key if it gave you one."),
        NoPage::CantSignIn(_) => format!("{why} It may take a key."),
    }
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

/// What to say when the hub couldn't do `what` for a server, such as "save it", or no longer has it.
pub(super) fn couldnt(what: &str, e: &ClientError) -> String {
    if e.is_not_found() {
        return refusals::gone("MCP server");
    }
    refusals::couldnt(what, e)
}
