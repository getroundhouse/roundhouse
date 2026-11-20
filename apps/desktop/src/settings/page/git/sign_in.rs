//! Signing in with GitHub: starting a sign-in, which the hub runs, cancelling it, and showing one that waits, wherever
//! it was started. How it stands, from the code to how it ended, is the hub's record of it, which every client shows
//! the same. The page keeps only which sign-in its panel shows, and which it has put away.

use client_core::records::Snapshot;
use gpui_kit::{Context, Window};
use protocol::model::{GithubSignIn, GithubSignInId};

use super::Asking;
use super::kept::{Kept, Shows};
use super::sign_in_words;

/// The panel of a sign-in with GitHub.
pub(super) struct SignInPanel {
    pub(super) id: GithubSignInId,
    /// It waits for the hub to answer its start, so the hub may have no record of it yet.
    pub(super) starting: bool,
    /// Why the hub couldn't start it, if it couldn't.
    pub(super) not_started: Option<String>,
    /// The username of the account it's to sign in as again, when it was started from that account's row.
    pub(super) again: Option<String>,
}

impl Kept {
    /// Has the hub start a sign-in with GitHub, shown in a panel under Added's heading, in place of any other. `again`
    /// is the username of the account to sign in as again, when it's started from that account's row.
    pub(super) fn sign_in(&mut self, again: Option<String>, cx: &mut Context<Self>) {
        let Some(asking) = self.asking.clone() else { return };
        if let Some(shown) = self.sign_in_panel() {
            self.put_away.insert(shown.id.clone());
        }
        let id = client_core::new_github_sign_in_id();
        let asked = id.clone();
        let starting = asking.client.start(move |client| client.github_sign_in_start(&asked));
        let started = id.clone();
        // Not with the panel, so a start whose panel goes before the hub answers is still cancelled once it has.
        cx.spawn(async move |this, cx| {
            let answer = starting.recv().await;
            let _ = this.update(cx, |this, cx| this.started(&started, answer, &asking, cx));
        })
        .detach();
        self.open(Shows::SignIn(SignInPanel { id, starting: true, not_started: None, again }), cx);
    }

    /// Takes in what the hub answered the start of `id` with: its record, which client-core has already, or why it
    /// couldn't start it, which only its own panel says. One whose panel was put away meanwhile, as when it was cancelled before the hub answered, is
    /// cancelled now, so the hub doesn't go on waiting for it.
    fn started(
        &mut self,
        id: &GithubSignInId,
        answer: client_core::Result<GithubSignIn>,
        asked: &Asking,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = self.sign_in_panel_mut().filter(|panel| &panel.id == id) {
            panel.starting = false;
            panel.not_started = answer.err().map(|e| sign_in_words::not_started(&e));
            cx.notify();
            return;
        }
        if answer.is_ok_and(|sign_in| sign_in.status.is_waiting()) && self.put_away.contains(id) {
            cancel_at_hub(id.clone(), asked);
        }
    }

    /// Cancels the sign-in the panel shows, which waits, or is starting, and puts the panel away. One that's starting
    /// is cancelled once the hub has answered its start.
    pub(super) fn cancel_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sign_in_panel() else { return };
        if !panel.starting
            && let Some(asking) = &self.asking
        {
            cancel_at_hub(panel.id.clone(), asking);
        }
        self.put_away_panel(window, cx);
    }

    /// Shows a sign-in that waits, whether it was started here or anywhere else, such as in another window or before
    /// the app last quit, while no panel but Connect account's is open, unless its panel was put away here.
    pub(super) fn show_waiting(&mut self, records: &Snapshot, cx: &mut Context<Self>) {
        if !matches!(self.panel.as_ref().map(|panel| &panel.shows), None | Some(Shows::Choosing(_))) {
            return;
        }
        let waiting = records.github_sign_ins().filter(|sign_in| sign_in.status.is_waiting());
        let Some(id) = waiting.map(|sign_in| &sign_in.id).filter(|id| !self.put_away.contains(*id)).min() else {
            return;
        };
        let panel = SignInPanel { id: id.clone(), starting: false, not_started: None, again: None };
        self.open(Shows::SignIn(panel), cx);
    }

    pub(super) fn sign_in_panel(&self) -> Option<&SignInPanel> {
        let Some(Shows::SignIn(sign_in)) = self.panel.as_ref().map(|panel| &panel.shows) else { return None };
        Some(sign_in)
    }

    fn sign_in_panel_mut(&mut self) -> Option<&mut SignInPanel> {
        let Some(Shows::SignIn(sign_in)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(sign_in)
    }
}

/// Has the hub cancel the sign-in `id`. The record it answers with, which client-core takes in, says how it stands
/// whatever that is, as when GitHub approved it meanwhile.
fn cancel_at_hub(id: GithubSignInId, asking: &Asking) {
    // Not waited for: what it says is in the records, and it runs to the end whatever the page does.
    drop(asking.client.start(move |client| client.github_sign_in_cancel(&id)));
}
