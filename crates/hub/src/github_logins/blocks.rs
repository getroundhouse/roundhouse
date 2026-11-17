//! Why the default's login can't be used, and what the block workers are sent then says, in the words git prints as it
//! refuses a push, which every client shows too.

use std::io;

use protocol::model::{GithubAccount, GithubUsername, UnusableLogin};

/// Why the login the hub holds for an account can't be used now.
pub(super) enum Unusable {
    /// It holds none, as once it was disconnected.
    Disconnected,
    /// Its token has ended, by GitHub's word.
    Ended,
    /// It's a sign-in's, whose token has ended, by GitHub's word, or that needs signing in again, as when GitHub has
    /// refused what renews it, so only signing in again gives it a login that works.
    SignInAgain,
    /// It's a sign-in's whose token has ended, by GitHub's word, which what renews it still can, as the hub tries to at
    /// each check, so it works again once that's done, as after a Mac wakes from a sleep past the token's end. Or an
    /// installation's whose token has ended, which the hub is making the next of.
    Renewing,
    /// Its file holds a login in a form only a newer build reads, so this one can't use it, and keeps it for that one.
    Newer,
    /// Its file holds no token, or, with the error, can't be read.
    Unreadable(Option<io::Error>),
}

impl Unusable {
    /// Why, as the hub's settings say it of a default that blocks.
    pub(super) fn said(&self) -> UnusableLogin {
        match self {
            Unusable::Disconnected => UnusableLogin::Disconnected,
            Unusable::Ended => UnusableLogin::Ended,
            Unusable::SignInAgain => UnusableLogin::SignInAgain,
            Unusable::Renewing => UnusableLogin::Renewing,
            Unusable::Newer => UnusableLogin::Newer,
            Unusable::Unreadable(_) => UnusableLogin::Unreadable,
        }
    }
}

/// What a block says, `why` agents push as nobody, and `to_do` about it. Git says it as it refuses to push, in a
/// terminal or an agent's output, where nothing else says it's Roundhouse's, so it names Roundhouse.
pub(super) fn reason(why: &str, to_do: &str) -> String {
    format!("{why}, so this workspace's agents push to GitHub as nobody. {to_do}")
}

/// What the block for a default, `account`, whose login can't be used, as `unusable` says, says. An app's installation
/// is named with where it's installed, and is connected again, and renewed, with the app.
pub(super) fn lost(account: &GithubAccount, unusable: &Unusable) -> String {
    let Some(installation) = &account.installation else { return lost_login(&account.user.name, unusable) };
    let name = format!("{}, installed on {}", account.user.name, installation.on.name);
    let elsewhere = "or choose another default in Roundhouse's Settings.";
    match unusable {
        Unusable::Disconnected => reason(
            &format!("Roundhouse's default GitHub account, {name}, was disconnected, or the app uninstalled there"),
            &format!(
                "Install the app there again, connect its installations again in Roundhouse's Settings, {elsewhere}"
            ),
        ),
        Unusable::Renewing => reason(
            &format!(
                "The token of Roundhouse's default GitHub account, {name}, has ended, and Roundhouse is making another"
            ),
            &format!(
                "It tries every minute until GitHub answers. If this goes on, check the app is still on GitHub, {elsewhere}"
            ),
        ),
        Unusable::Ended | Unusable::SignInAgain | Unusable::Newer | Unusable::Unreadable(_) => {
            lost_login(&name.into(), unusable)
        }
    }
}

/// What the block for a default, `name`, whose login can't be used, as `unusable` says, says.
fn lost_login(name: &GithubUsername, unusable: &Unusable) -> String {
    let connect_again = "Connect it again in Roundhouse's Settings, or choose another default there.";
    match unusable {
        Unusable::Disconnected => {
            reason(&format!("Roundhouse's default GitHub account, {name}, was disconnected"), connect_again)
        }
        Unusable::Ended => reason(
            &format!("The token of Roundhouse's default GitHub account, {name}, has ended"),
            "Renew it in Roundhouse's Settings, or choose another default there.",
        ),
        Unusable::SignInAgain => reason(
            &format!("The sign-in of Roundhouse's default GitHub account, {name}, has ended"),
            "Sign in with GitHub again in Roundhouse's Settings, or choose another default there.",
        ),
        Unusable::Renewing => reason(
            &format!(
                "The token of Roundhouse's default GitHub account, {name}, has ended, and Roundhouse is renewing it"
            ),
            "It tries every minute until GitHub answers. If this goes on, sign in with GitHub again in Roundhouse's \
             Settings.",
        ),
        Unusable::Newer => reason(
            &format!(
                "The login of Roundhouse's default GitHub account, {name}, is one only a newer Roundhouse can read"
            ),
            "Update Roundhouse, connect the account again in its Settings, or choose another default there.",
        ),
        Unusable::Unreadable(None) => {
            reason(&format!("Roundhouse can't find the token of its default GitHub account, {name}"), connect_again)
        }
        Unusable::Unreadable(Some(e)) => reason(
            &format!("Roundhouse can't read the token of its default GitHub account, {name} ({e})"),
            connect_again,
        ),
    }
}
