//! What Git keeps from one frame to the next while it shows: the panel that's open, the sign-ins put away here, and what
//! the hub last couldn't do. Which account is the default, how each login stands and how a sign-in goes are never kept
//! here: each frame reads them from the hub's records and settings, as client-core keeps them.

use protocol::client::github_accounts::{GithubAccountDisconnect, GithubAccountSetDefault};
use std::collections::HashSet;

use client_core::records::Snapshot;
use gpui_kit::{Bounds, Context, FocusHandle, Pixels, ScrollHandle, Task, Window};
use protocol::model::{GithubAccountId, GithubAppId, GithubSignInId};

use super::super::panel::Place;
use super::super::redraw::RedrawAt;
use super::Asking;
use super::app_panel::AppPanel;
use super::gh_choice::ChoosingPanel;
use super::sign_in::SignInPanel;
use super::token::TokenPanel;
use super::words;
use crate::asking::{self, Question};

/// The page's state, kept only while it shows, so a token half pasted goes with it, and so does the wait for what the
/// hub answers.
#[derive(Default)]
pub(super) struct Kept {
    /// The hub to ask, as of the page's last frame, with client-core's copy of its records. `None` while there's no hub
    /// to ask.
    pub(super) asking: Option<Asking>,
    /// Settings' focus, as of the page's last frame, where focus goes back to as a panel goes while it has it, so keys
    /// still reach Settings.
    settings_focus: Option<FocusHandle>,
    pub(super) panel: Option<Panel>,
    /// The sign-ins whose panel was put away here, so one still waiting isn't shown again while the page shows.
    pub(super) put_away: HashSet<GithubSignInId>,
    /// Why the hub couldn't do what was last asked of an account's row, or of Found, to say there.
    pub(super) failed: Option<(Asked, String)>,
    /// The wait for the hub to choose the default, while it hasn't answered.
    choosing: Option<Task<()>>,
    /// Draws the page again as a login it shows ends.
    redraw: RedrawAt,
}

/// Where what the hub couldn't do is said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Asked {
    /// Under the account's row: disconnecting it, making it the default, or, for an app's installation, connecting it
    /// again or removing the app.
    Account(GithubAccountId),
    /// Under the row of an app of your own that's installed nowhere: looking for where it's installed, or removing it.
    App(GithubAppId),
}

/// A panel open on the page: under Added's heading, or under the row of the account it renews.
pub(super) struct Panel {
    pub(super) shows: Shows,
    pub(super) place: Place,
}

pub(super) enum Shows {
    /// Connect account, choosing how to connect one.
    Choosing(ChoosingPanel),
    Token(TokenPanel),
    SignIn(SignInPanel),
    /// Making an app of your own, or installing one.
    App(AppPanel),
}

impl Panel {
    /// The account whose row it's under, rather than under Added's heading: one whose token it renews.
    pub(super) fn under(&self) -> Option<&GithubAccountId> {
        match &self.shows {
            Shows::Token(token) => token.renewing.as_ref().map(|renewing| &renewing.id),
            Shows::Choosing(_) | Shows::SignIn(_) | Shows::App(_) => None,
        }
    }
}

impl Kept {
    /// What the page was drawn with: the hub to ask, Settings' focus, the hub's records, and when what it says of a time
    /// next changes, if it says one.
    pub(super) fn drawn(
        &mut self,
        asking: Option<Asking>,
        settings_focus: FocusHandle,
        records: &Snapshot,
        changes_at_ms: Option<i64>,
        cx: &mut Context<Self>,
    ) {
        (self.asking, self.settings_focus) = (asking, Some(settings_focus));
        self.redraw.set(changes_at_ms, cx);
        self.show_waiting(records, cx);
    }

    /// Whether there's a hub to ask.
    pub(super) fn can_ask(&self) -> bool {
        self.asking.is_some()
    }

    /// Why the hub couldn't do what was last asked `at`, if it couldn't.
    pub(super) fn failed(&self, at: &Asked) -> Option<&str> {
        self.failed.as_ref().filter(|(asked, _)| asked == at).map(|(_, said)| said.as_str())
    }

    /// Gives the panel itself the focus, when something in it had it, as what had it goes, so keys still reach Settings.
    pub(super) fn focus_the_panel(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &self.panel {
            panel.place.keep_focus(window, cx);
        }
    }

    /// Opens a panel showing `shows`, in place of any that was open.
    pub(super) fn open(&mut self, shows: Shows, cx: &mut Context<Self>) {
        self.panel = Some(Panel { shows, place: Place::opening(cx) });
        self.failed = None;
        cx.notify();
    }

    /// Takes the panel away, and any wait for the hub's answer with it, putting away the sign-in it showed. Its fields
    /// go too, so focus one of them had, or anything else in it, goes back to Settings, where keys still reach it.
    pub(super) fn put_away_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.panel.take() else { return };
        if let Shows::SignIn(sign_in) = &panel.shows {
            self.put_away.insert(sign_in.id.clone());
        }
        if let Some(settings) = &self.settings_focus {
            panel.place.give_focus(settings, window, cx);
        }
        cx.notify();
    }

    /// The panel was laid out at `bounds`, on Settings' `scroll`, as [`Place::laid_out`] says.
    pub(super) fn laid_out(&mut self, bounds: Bounds<Pixels>, scroll: &ScrollHandle, window: &mut Window) {
        if let Some(panel) = &mut self.panel {
            panel.place.laid_out(bounds, scroll, window);
        }
    }

    /// Has the hub make `account` the default, and says under its row why it couldn't, if it couldn't. The hub's
    /// settings say which is the default, once it has answered.
    pub(super) fn make_default(&mut self, account: GithubAccountId, cx: &mut Context<Self>) {
        let at = Asked::Account(account.clone());
        let Some(asking) = self.asking.clone() else {
            self.failed = Some((at, words::NO_HUB.to_string()));
            cx.notify();
            return;
        };
        self.failed = None;
        let choosing = asking.hub.ask(GithubAccountSetDefault { account: Some(account) });
        self.choosing = Some(cx.spawn(async move |this, cx| {
            let answer = choosing.recv().await;
            let _ = this.update(cx, |this, cx| {
                this.choosing = None;
                if let Err(e) = answer {
                    this.failed = Some((at, words::couldnt("make it the default", &e)));
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    /// Asks whether to have the hub forget the login of the account `id`, called `name`, saying what that `says`, and
    /// does once it's confirmed.
    pub(super) fn ask_to_disconnect(
        &mut self,
        id: GithubAccountId,
        name: &str,
        says: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.failed = None;
        let asked = Question {
            id: "disconnect-github-account",
            title: words::disconnect_asks(name).into(),
            says: says.into(),
            keep: "Cancel",
            act: "Disconnect",
            removes: true,
        };
        let this = cx.weak_entity();
        asking::ask(asked, window, cx, move |_, cx| {
            let _ = this.update(cx, |this, cx| this.disconnect(id.clone(), cx));
        });
    }

    /// Has the hub forget the login of the account `id`, and says under its row why it couldn't, if it couldn't, as
    /// with no hub to ask.
    pub(super) fn disconnect(&mut self, id: GithubAccountId, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else {
            self.failed = Some((Asked::Account(id), words::NO_HUB.to_string()));
            cx.notify();
            return;
        };
        let disconnecting = asking.hub.ask(GithubAccountDisconnect { id: id.clone() });
        cx.spawn(async move |this, cx| {
            let answer = disconnecting.recv().await;
            let _ = this.update(cx, |this, cx| {
                // An account that's gone was disconnected already, as by another app, which is what was asked.
                if let Err(e) = answer
                    && !e.is_not_found()
                {
                    this.failed = Some((Asked::Account(id), words::couldnt("disconnect it", &e)));
                }
                cx.notify();
            });
        })
        .detach();
    }
}
