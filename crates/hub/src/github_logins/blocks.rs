//! Why the default's login can't be used, and what the block workers are sent then says, in the words git prints as it
//! refuses a push, which every client shows too, as Git's box says it.

use std::io;

use protocol::model::{GithubAccount, GithubUsername, UnusableLogin};

use super::gh_login::{HUB_MACHINE, NoGhLogin};

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
    /// It's the login `gh` has on the hub's machine, and `gh` there has no login that works, as the reason says: it's
    /// logged out, there's no `gh`, or GitHub refused its token.
    GhLoggedOut(NoGhLogin),
    /// It's the login `gh` has on the hub's machine, which is logged in as this other GitHub user now.
    GhOtherUser(GithubUsername),
    /// It's the login `gh` has on the hub's machine, which the hub hasn't read lately, with why, if a read failed.
    GhUnread(Option<String>),
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
            Unusable::GhLoggedOut(_) => UnusableLogin::GhLoggedOut,
            Unusable::GhOtherUser(_) => UnusableLogin::GhOtherUser,
            Unusable::GhUnread(_) => UnusableLogin::GhUnread,
        }
    }
}

/// What a block says: `what` is wrong, what can't be done `until` it's put right, and `to_do` about it, each said once.
/// Git says it as it refuses to push, in a terminal or an agent's output on any provider Roundhouse sets up GitHub on,
/// and Git's box says it on whichever device the person is, so it names Roundhouse, and never says where they are, as
/// "this workspace" or "there" for the hub's machine would. *Unconfirmed:* the words.
pub(super) fn reason(what: &str, until: &str, to_do: &str) -> String {
    format!(
        "{what}. Until {until}, Roundhouse can't read pull requests, and agents on the providers it sets up can't push. \
         {to_do}"
    )
}

/// What the block while the hub has no default says: that Roundhouse has no GitHub account, and to connect one, or,
/// while it has accounts, as once the default was disconnected, that none is the default, and to choose one.
/// *Unconfirmed:* the words.
pub(super) fn no_account(has_accounts: bool) -> String {
    if has_accounts {
        return reason(
            "Roundhouse has no default GitHub account",
            "one is chosen",
            "Make one the default in Roundhouse's Settings, or connect another.",
        );
    }
    reason(
        "Roundhouse has no GitHub account",
        "one is connected",
        "Connect one in Roundhouse's Settings, such as gh's login on the machine Roundhouse runs on.",
    )
}

/// What the block for a default, `account`, whose login can't be used, as `unusable` says, says. An app's installation
/// is named with where it's installed, and is connected again, and renewed, with the app.
pub(super) fn lost(account: &GithubAccount, unusable: &Unusable) -> String {
    let Some(installation) = &account.installation else { return lost_login(&account.user.name, unusable) };
    let (name, on) = (&account.user.name, &installation.on.name);
    match unusable {
        Unusable::Disconnected => reason(
            &format!(
                "{name}, the default account, installed on {on}, was disconnected, or the app uninstalled from {on}"
            ),
            "it's connected again",
            &format!("Install the app on {on} again and connect its installation, or choose another default."),
        ),
        Unusable::Renewing => reason(
            &format!(
                "The token of {name}, the default account, installed on {on}, has ended, and Roundhouse is making \
                 another"
            ),
            "it has",
            "It tries every minute until GitHub answers. If this goes on, check the app is still on GitHub, or choose \
             another default.",
        ),
        Unusable::Ended
        | Unusable::SignInAgain
        | Unusable::Newer
        | Unusable::Unreadable(_)
        | Unusable::GhLoggedOut(_)
        | Unusable::GhOtherUser(_)
        | Unusable::GhUnread(_) => lost_login(name, unusable),
    }
}

/// What the block for a default, `name`, whose login can't be used, as `unusable` says, says.
fn lost_login(name: &GithubUsername, unusable: &Unusable) -> String {
    let connect_again = "Connect it again, or choose another default.";
    match unusable {
        Unusable::Disconnected => {
            reason(&format!("{name}, the default account, was disconnected"), "it's connected again", connect_again)
        }
        Unusable::Ended => reason(
            &format!("The token of {name}, the default account, has ended"),
            "it's renewed",
            "Renew it, or choose another default.",
        ),
        Unusable::SignInAgain => reason(
            &format!("The sign-in of {name}, the default account, has ended"),
            "it's signed in again",
            "Sign in with GitHub again, or choose another default.",
        ),
        Unusable::Renewing => reason(
            &format!("The token of {name}, the default account, has ended, and Roundhouse is renewing it"),
            "it has",
            "It tries every minute until GitHub answers. If this goes on, sign in with GitHub again.",
        ),
        Unusable::Newer => reason(
            &format!("The login of {name}, the default account, is one only a newer Roundhouse can read"),
            "Roundhouse is updated or it's connected again",
            "Update Roundhouse, connect the account again, or choose another default.",
        ),
        Unusable::Unreadable(None) => reason(
            &format!("Roundhouse can't find the token of {name}, the default account"),
            "it's connected again",
            connect_again,
        ),
        Unusable::Unreadable(Some(e)) => reason(
            &format!("Roundhouse can't read the token of {name}, the default account ({e})"),
            "it's connected again",
            connect_again,
        ),
        Unusable::GhLoggedOut(why) => reason(
            &format!("{name}, the default account, is gh's login, and {why}"),
            &format!("gh is logged in as {name} again"),
            "Run gh auth login, or choose another default.",
        ),
        Unusable::GhOtherUser(now) => reason(
            &format!("{name}, the default account, is gh's login on {HUB_MACHINE}, which is {now} now"),
            &format!("it's {name} again"),
            &format!("Run gh auth switch --user {name}, connect {now}, or choose another default."),
        ),
        Unusable::GhUnread(why) => reason(&gh_unread(name, why.as_deref()), "it can", "It asks gh again every minute."),
    }
}

/// Why a default, `name`, that's `gh`'s login can't be used while the hub hasn't read `gh` lately, for the reason `why`
/// a read failed, if one did.
fn gh_unread(name: &GithubUsername, why: Option<&str>) -> String {
    let unread = format!("{name}, the default account, is gh's login on {HUB_MACHINE}");
    match why {
        Some(why) => format!("{unread}, which Roundhouse couldn't read lately ({why})"),
        None => format!("{unread}, which Roundhouse hasn't read yet"),
    }
}
