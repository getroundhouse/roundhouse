//! What Git says of apps of your own: the way to connect one in Connect account's panel, each step of the panel that
//! makes one, what GitHub and the hub refused there, as far as it's particular to an app, and what an app's rows and
//! their menus say.

use client_core::records::Snapshot;
use client_core::{ClientError, refusals};
use protocol::client::ErrorCode;
use protocol::model::{GhLogin, GithubAccountId, GithubApp};

use super::added::listed;
use crate::manifest_flow::{NotAnOrganization, Under};

/// What Connect account's panel says of an app of your own.
pub(super) const CHOICE_SAYS: &str = "Make a GitHub App under your account or an organization, then install it on \
                                      the repositories agents should reach. Only the hub holds its key.";

/// What a row says of how an installation was connected.
pub(super) const APP: &str = "App";

/// What an app's row says while it's installed nowhere.
pub(super) const NOT_INSTALLED: &str = "Not installed";

/// What the field for an organization's name says under it.
pub(super) const ORGANIZATION_SAYS: &str =
    "As in its address on GitHub. Only its owners and app managers can make an app under it.";

/// What the field for the app's name says under it.
pub(super) const NAME_SAYS: &str = "No other app on GitHub can have it. GitHub names its bot after it.";

/// The steps' titles, before each is done.
pub(super) const CHOOSE: &str = "Choose where to make it, and its name";
pub(super) const MAKE: &str = "Make it on GitHub";
pub(super) const INSTALL: &str = "Install it on the repositories agents should reach";

/// What making it on GitHub says while the hub writes its manifest.
pub(super) const OPENING: &str = "Opening GitHub…";

/// What making it on GitHub says while the hub takes the app's key from GitHub.
pub(super) const MAKING: &str = "Taking its key from GitHub…";

/// What installing it says while it waits.
pub(super) const INSTALL_SAYS: &str = "Choose the repositories on GitHub. Roundhouse looks for it as you come back \
                                       here, and the hub looks every minute.";

/// What the panel says of an app the hub no longer has, as once it's been removed elsewhere.
pub(super) fn gone() -> String {
    refusals::gone("app")
}

/// What installing it says while the hub looks.
pub(super) const LOOKING: &str = "Looking for it on GitHub…";

/// The panel's title, once the app is `installed` or before.
pub(super) fn title(installed: bool) -> &'static str {
    if installed { "Made an app of your own" } else { "Make an app of your own" }
}

/// What the panel says under its title: who agents act as, by its `bot`'s name once GitHub has said it.
pub(super) fn lead(bot: Option<&str>) -> String {
    match bot {
        Some(bot) => format!("Agents act as its bot, {bot}, rather than as you. Only the hub holds its key."),
        None => "Agents act as its bot, rather than as you. Only the hub holds its key.".to_string(),
    }
}

/// What the panel says under its title once `bot` is installed `on` an account or organization, which is the
/// default when `default`.
pub(super) fn lead_installed(bot: &str, on: &str, default: bool) -> String {
    if default {
        format!(
            "{bot} is installed on {on}. It's the default, so agents push as it wherever Roundhouse sets up GitHub."
        )
    } else {
        format!("{bot} is installed on {on}. Make it the default from its menu for agents to push as it.")
    }
}

/// What choosing where says once it's done: `under` the account or organization, with `name`.
pub(super) fn chosen(under: &Under, name: &str) -> String {
    match under {
        Under::Account => format!("Under your account, as {name}"),
        Under::Organization(organization) => format!("Under {}, as {name}", organization.as_str()),
    }
}

/// What the first two steps say once GitHub has made `app`.
pub(super) fn made(app: &GithubApp) -> [String; 2] {
    [format!("Under {}, as {}", app.owner.name, app.name), format!("Made {} on GitHub", app.slug)]
}

/// What installing it says once it's installed `on` an account or organization.
pub(super) fn installed_on(on: &str) -> String {
    format!("Installed on {on}")
}

/// What making it on GitHub says while GitHub hasn't sent the person back, `until` when the app stops waiting.
pub(super) fn on_github(until: Option<&str>) -> [String; 2] {
    let says = "Make it on the page your browser opened, where you can change its name if another app has it. \
                GitHub sends you back here once it's made.";
    let waiting = match until {
        Some(until) => format!("Waiting for GitHub to send you back, until {until}."),
        None => "Waiting for GitHub to send you back.".to_string(),
    };
    [says.to_string(), waiting]
}

/// What making it on GitHub says once the app stopped waiting for GitHub, after `minutes`.
pub(super) fn stopped_waiting(minutes: u64) -> String {
    format!("Roundhouse stopped waiting for GitHub after {minutes} minutes. Open GitHub again to make it.")
}

/// Why an organization can't be made under, as typed.
pub(super) fn not_an_organization(not: &NotAnOrganization) -> String {
    if not.0.is_empty() {
        return "Type the organization's name.".to_string();
    }
    format!("{} isn't an organization's name. GitHub's have only letters, numbers and single hyphens.", not.0)
}

/// Why the app couldn't listen for GitHub sending the person back.
pub(super) fn couldnt_listen(e: &std::io::Error) -> String {
    format!("Couldn't listen for GitHub on this {}: {e}", crate::names::MACHINE)
}

/// Why the hub wouldn't write the app's manifest.
pub(super) fn not_written(e: &ClientError) -> String {
    match e {
        ClientError::Api(api) if api.code == ErrorCode::InvalidRequest => {
            format!("Choose another name: {}.", api.message)
        }
        e if e.is_unsupported() => "This hub can't make an app of your own until it's updated.".to_string(),
        e => refusals::couldnt("open GitHub", e),
    }
}

/// Why the hub couldn't make the app from GitHub's code, and whether giving it the same code again might.
pub(super) fn not_made(e: &ClientError) -> (String, bool) {
    match e {
        ClientError::Api(api) if api.code == ErrorCode::InvalidRequest => (
            "GitHub no longer takes the code it sent you back with, so the hub has no key for the app it made. Delete \
             the app on GitHub, in your Developer settings, or choose another name, and start again."
                .to_string(),
            false,
        ),
        e if e.is_unsupported() => ("This hub can't make an app of your own until it's updated.".to_string(), false),
        e => (
            format!(
                "The hub couldn't take the app's key from GitHub: {}. GitHub takes its code for an hour, so try again.",
                refusals::reason(e)
            ),
            true,
        ),
    }
}

/// What to say when the hub couldn't do `what` for an app, such as "remove it", or no longer has it.
pub(super) fn couldnt(what: &str, e: &ClientError) -> String {
    if e.is_not_found() {
        return gone();
    }
    refusals::couldnt(what, e)
}

/// What to say when the hub couldn't look for where an app's installed.
pub(super) fn not_looked(e: &ClientError) -> String {
    couldnt("look for it on GitHub", e)
}

/// What removing the app called `name` asks first.
pub(super) fn remove_asks(name: &str) -> String {
    format!("Remove {name}?")
}

/// What the question before removing an app says under it, and more when its installation is the `default`.
pub(super) fn remove_says(default: bool) -> &'static str {
    if default {
        "The hub forgets its key and its installation's login, so agents can't act as its bot. Nothing is deleted on \
         GitHub: delete the app there to be rid of it. Its installation stays the default, so agents can't push until \
         you make another account the default."
    } else {
        "The hub forgets its key and its installation's login, so agents can't act as its bot. Nothing is deleted on \
         GitHub: delete the app there to be rid of it."
    }
}

/// What the question before disconnecting an installation says under it, and more when it's the `default`.
pub(super) fn disconnect_says(default: bool) -> &'static str {
    if default {
        "The hub forgets its token, and makes no more until you connect it again. The app stays installed on GitHub. \
         It stays the default, so agents can't push until you connect it again or make another account the default."
    } else {
        "The hub forgets its token, and makes no more until you connect it again. The app stays installed on GitHub."
    }
}

/// The name an app is first given: the username of the first account added that's someone's rather than an app's, in
/// the order Added lists them, or else of the first `gh` login found, with `-roundhouse` after it, or nothing when
/// there's neither.
pub(super) fn default_name(records: &Snapshot, default: Option<&GithubAccountId>) -> String {
    let added = listed(records.github_accounts(), default).into_iter().find(|account| account.installation.is_none());
    let found = records.found_github_logins().iter().find_map(|found| match &found.login {
        GhLogin::LoggedIn { username, .. } => Some(username.to_string()),
        GhLogin::LoggedOut | GhLogin::NotInstalled | GhLogin::Unknown => None,
    });
    let name = added.map(|account| account.user.name.to_string()).or(found);
    name.map_or_else(String::new, |name| format!("{name}-roundhouse"))
}
