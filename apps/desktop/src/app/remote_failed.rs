//! What shows in place of a workspace while a paired hub can't be used: why, and what to do about it.

use client_core::OlderSide;
use client_ui::needs_updating;
use gpui_kit::{AnyElement, Context, IntoElement as _};

use super::view::{switch_to_local, try_again};
use super::{AppView, ConnectionFailure, pairing};
use crate::names::MACHINE;
use crate::updates::{self, OwnUpdate};

/// What's said of a paired hub that can't be used, and the main thing to do about it.
struct Failed {
    heading: String,
    says: String,
    then: Then,
}

/// The main button's, beside the one that switches to this machine's hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Then {
    TryAgain,
    PairAgain,
    RestartToUpdate,
    /// Checks for an update of this app, which `checking` says is under way.
    CheckForUpdates {
        checking: bool,
    },
}

impl AppView {
    pub(super) fn remote_failed(&self, failure: &ConnectionFailure, cx: &mut Context<Self>) -> AnyElement {
        let hub_name = self.hub.remote.as_ref().map_or("the hub", |hub| hub.hub_name.as_str());
        let own = updates::own_update(cx);
        let Failed { heading, says, then } = words(hub_name, self.hub.build_version, failure, &own);
        look::empty(look::EmptyKind::Failed, heading)
            .says(says)
            .answer(switch_to_local("failed-switch-local"))
            .answer(self.main_button(then, cx))
            .into_any_element()
    }

    fn main_button(&self, then: Then, cx: &mut Context<Self>) -> look::Button {
        let button =
            |id: &'static str, words| look::button(id, look::ButtonKind::Main, look::ControlSize::Large, words);
        match then {
            Then::TryAgain => try_again(cx),
            Then::PairAgain => {
                let data_dir = self.hub.paths.data_dir.clone();
                button("pair-hub-again", "Pair again…")
                    .on_click(move |_, window, cx| pairing::open(data_dir.clone(), window, cx))
            }
            Then::RestartToUpdate => button("failed-restart-to-update", "Restart to update")
                .on_click(cx.listener(|t, _, _, cx| t.restart_to_update(cx))),
            Then::CheckForUpdates { checking } => button("failed-check-for-updates", "Check for updates")
                .waiting(checking)
                .on_click(|_, _, cx| updates::check_now(cx)),
        }
    }
}

#[cfg(any(test, feature = "testing"))]
impl AppView {
    /// Shows `failure` of `hub`, as if the app had tried to connect to it, which takes another machine's hub for real.
    pub fn show_failed(&mut self, hub: client_core::RemoteHub, failure: ConnectionFailure, cx: &mut Context<Self>) {
        // The link is still this machine's hub's, so what it says next would show in its place.
        self.following = None;
        self.hub.remote = Some(hub);
        self.set_connection(super::Connection::Failed(failure), cx);
        cx.notify();
    }
}

/// What's said of `failure` of the paired hub called `hub_name`, by an app of the release `app`, whose own update
/// stands as `own` says.
fn words(hub_name: &str, app: &str, failure: &ConnectionFailure, own: &OwnUpdate<'_>) -> Failed {
    match failure {
        ConnectionFailure::Unreachable { hosts } => {
            let at = if hosts.is_empty() { String::new() } else { format!(" at {}", hosts.join(" or ")) };
            Failed {
                heading: format!("Can’t reach {hub_name}"),
                says: format!(
                    "Couldn’t connect{at}. Retrying automatically — check that {hub_name} is awake and on the same \
                     network or VPN."
                ),
                then: Then::TryAgain,
            }
        }
        ConnectionFailure::Revoked => Failed {
            heading: format!("Access to {hub_name} was revoked"),
            says: format!(
                "This computer’s device credential is no longer accepted by {hub_name}. Create a new pairing link \
                 there to reconnect."
            ),
            then: Then::PairAgain,
        },
        ConnectionFailure::KeyChanged { host } => Failed {
            heading: format!("{hub_name} presented a different key"),
            says: format!(
                "{host} no longer matches its saved key. Check that you’re reaching {hub_name} before pairing again."
            ),
            then: Then::PairAgain,
        },
        ConnectionFailure::RemoteProtocolBreak(older) => across_a_break(hub_name, app, older, own),
        ConnectionFailure::RemoteOther(detail) | ConnectionFailure::Local(detail) => {
            Failed { heading: format!("Couldn’t connect to {hub_name}"), says: detail.clone(), then: Then::TryAgain }
        }
    }
}

/// Where the app is the older side, updating it is the main thing to do, as far as it has got.
fn across_a_break(hub_name: &str, app: &str, older: &OlderSide, own: &OwnUpdate<'_>) -> Failed {
    let said = needs_updating(hub_name, MACHINE, Some(app), older);
    let (more, then) = match (older, own) {
        (OlderSide::Hub { .. }, _) => (None, Then::TryAgain),
        (OlderSide::Client { .. }, OwnUpdate::Ready(next)) => {
            (Some(format!("Roundhouse {next} is downloaded.")), Then::RestartToUpdate)
        }
        (OlderSide::Client { .. }, OwnUpdate::Checking) => (None, Then::CheckForUpdates { checking: true }),
        (OlderSide::Client { .. }, OwnUpdate::NothingNewer) => {
            (Some(format!("No newer release was found for this {MACHINE}.")), Then::CheckForUpdates { checking: false })
        }
        (OlderSide::Client { .. }, OwnUpdate::NotFound) => (None, Then::CheckForUpdates { checking: false }),
        (OlderSide::Client { .. }, OwnUpdate::Never) => {
            (Some(format!("Update Roundhouse on this {MACHINE} to reconnect.")), Then::TryAgain)
        }
    };
    let says = match more {
        Some(more) => format!("{} {more}", said.says),
        None => said.says,
    };
    Failed { heading: said.heading, says, then }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What's done about a break, for each side being older and however far the app's own update has got.
    #[test]
    fn the_app_offers_its_own_update_only_while_it_is_the_older_side() {
        let app = OlderSide::Client { hub_version: "0.1.9".to_string() };
        let hub = OlderSide::Hub { version: Some("0.1.1".to_string()) };
        let cases = [
            (&app, OwnUpdate::Ready("0.1.9"), Then::RestartToUpdate),
            (&app, OwnUpdate::Checking, Then::CheckForUpdates { checking: true }),
            (&app, OwnUpdate::NothingNewer, Then::CheckForUpdates { checking: false }),
            (&app, OwnUpdate::NotFound, Then::CheckForUpdates { checking: false }),
            (&app, OwnUpdate::Never, Then::TryAgain),
            (&hub, OwnUpdate::Ready("0.1.9"), Then::TryAgain),
        ];

        for (older, own, then) in cases {
            let failure = ConnectionFailure::RemoteProtocolBreak(older.clone());
            assert_eq!(words("Studio", "0.1.5", &failure, &own).then, then, "{older:?} with {own:?}");
        }
    }
}
