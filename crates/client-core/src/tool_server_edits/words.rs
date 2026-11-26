//! What's said of what's typed of a server before it's sent: what running agents will have of an edit, as the table of
//! what each change does in `docs/spec/settings.md` has it, and what saving it takes, under the way the server is to
//! know the person and under its key.

use super::edit::{Takes, Url};
use super::{Edit, Way};

/// What signing in to a server to save an edit of it does.
pub const SIGN_IN_TO_SAVE: &str = "Your browser opens on its own page. Nothing changes until you've signed in.";

/// What's said under the way the server is to know the person while `way` is chosen, for a server to add, or for
/// `edit`.
pub fn way_says(way: Way, edit: Option<&Edit>) -> &'static str {
    if let Some(edit) = edit {
        match (edit.takes, edit.url) {
            (Takes::StayingSignedIn, _) => return "You stay signed in.",
            (Takes::SignIn, Url::Moved) => {
                return "That's another server, so you sign in to it. Nothing changes until you have.";
            }
            (Takes::SignIn, Url::AsItWas | Url::AnotherQuery | Url::Invalid) => return SIGN_IN_TO_SAVE,
            (Takes::KeyOrTheKept | Takes::WhatsTyped, _) => {}
        }
    }
    match way {
        Way::SignIn => "Your browser opens on its own page, and the hub renews the sign-in by itself.",
        Way::Key => "Sent in a header with every request, such as an API key. Only the hub keeps it.",
        Way::Neither => "For a server that anyone who can reach it can use.",
    }
}

/// What's said under a key's field, for a server to add, or for `edit`.
pub fn key_says(edit: Option<&Edit>) -> &'static str {
    match edit {
        Some(edit) if edit.keeps_a_key() => "Leave it empty to keep the one you gave.",
        Some(edit) if edit.url == Url::Moved => "That's another server, so give it its key.",
        Some(_) | None => "What the server gave you.",
    }
}

/// What running agents have of `edit`, saved `anyway` if the server didn't list its tools, which a client says before
/// it's saved. The agents that can list a server's tools again are those whose run claims `tool_server_reload`, which
/// the words don't name, since which they are is each run's to say. *Unconfirmed.*
pub fn edit_says(edit: &Edit, anyway: bool) -> &'static str {
    let (moved, query) = match edit.url {
        Url::Moved => (true, false),
        Url::AnotherQuery => (false, true),
        Url::AsItWas | Url::Invalid => (false, false),
    };
    // As the hub has them, where its tools may be others: by another query, or none, where it's saved though it didn't
    // list them.
    let lists_again = query || (edit.changes_auth() && anyway);
    let rest = query || edit.changes_auth();
    match (edit.signs_in(), moved, lists_again, rest, edit.changes_name()) {
        (true, true, ..) => {
            "Once you've signed in, running agents that can list its tools again lose this server until their turn \
             ends, and others until their next start."
        }
        (false, true, ..) => {
            "Running agents that can list its tools again lose this server until their turn ends, and others until \
             their next start."
        }
        (true, false, true, _, true) => {
            "Once you've signed in, running agents have the change at once, and the new name from their next start. \
             Those that can list its tools again do so once their turn ends, and others at their next start."
        }
        (true, false, true, _, false) => {
            "Once you've signed in, running agents have the change at once. Those that can list its tools again do so \
             once their turn ends, and others at their next start."
        }
        (true, false, false, _, true) => {
            "Once you've signed in, running agents have the change at once, and the new name from their next start."
        }
        (true, false, false, _, false) => "Once you've signed in, running agents have the change at once.",
        (false, false, true, _, true) => {
            "Running agents have the new name from their next start, and the rest at once. Those that can list its \
             tools again do so once their turn ends, and others at their next start."
        }
        (false, false, true, _, false) => {
            "Running agents have the change at once. Those that can list its tools again do so once their turn ends, \
             and others at their next start."
        }
        (false, false, false, true, true) => {
            "Running agents have the new name from their next start, and the rest at once."
        }
        (false, false, false, true, false) => "Running agents have the change at once.",
        (false, false, false, false, true) => "Running agents have the new name from their next start.",
        (false, false, false, false, false) => "Change its name, its URL or how it knows you.",
    }
}
