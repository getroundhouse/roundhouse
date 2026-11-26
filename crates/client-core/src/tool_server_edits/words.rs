//! What's said of what's typed of a server before it's sent: what saving it takes, under the way the server is to know
//! the person and under its key.

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
            (Takes::SignIn, Url::SameServer) => return SIGN_IN_TO_SAVE,
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
