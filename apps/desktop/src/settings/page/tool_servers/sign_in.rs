//! Signing in to a server, as the page has a part in it: starting one, and showing the one there is, wherever it was
//! started. How it stands is the window's, in `crate::tool_sign_in`, so it goes on while the page doesn't show. A sign-in
//! to a new server that came to nothing is the panel it was started from again, with what was typed in it and why, so it
//! can be changed there, and tried again or added another way.

use gpui_kit::{Context, Window};
use protocol::model::ToolServer;

use super::kept::{Kept, Shows};
use super::words;
use crate::tool_sign_in::{NewServer, SignInTo, Stands, ToolSigningIn, UnderWay};

impl Kept {
    /// Signs in to `to`, in a panel under Added's heading, in place of any other.
    pub(super) fn sign_in(&mut self, to: SignInTo, window: &mut Window, cx: &mut Context<Self>) {
        self.put_away_panel(window, cx);
        self.signing_in.update(cx, |signing_in, cx| signing_in.start(to, cx));
    }

    /// Signs in to `server` again.
    pub(super) fn sign_in_again(&mut self, server: &ToolServer, window: &mut Window, cx: &mut Context<Self>) {
        self.sign_in(SignInTo::again(server), window, cx);
    }

    /// Ends the sign-in there is, which takes its panel away.
    pub(super) fn put_away_sign_in(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.signing_in.update(cx, ToolSigningIn::put_away);
    }

    /// Opens the page of the sign-in there is again.
    pub(super) fn open_sign_in_page(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.signing_in.update(cx, |signing_in, cx| signing_in.open_page(cx));
    }

    /// Tries the sign-in there is again, from its start.
    pub(super) fn try_sign_in_again(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(to) = self.signing_in.read(cx).under_way().map(|under_way| under_way.to().clone()) {
            self.sign_in(to, window, cx);
        }
    }

    /// Has the page show how the sign-in there is stands now: its panel, for one that has come to be, and no panel once
    /// it has gone.
    pub(super) fn follow_sign_in(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = matches!(self.panel.as_ref().map(|panel| &panel.shows), Some(Shows::SigningIn));
        let signing_in = self.signing_in.read(cx);
        let Some(under_way) = signing_in.under_way() else {
            if shown {
                self.put_away_panel(window, cx);
            }
            return;
        };
        if let Some((typed, why)) = not_added(under_way) {
            self.put_away_panel(window, cx);
            self.open_sign_in_refused(typed, why, window, cx);
            self.put_away_sign_in(window, cx);
        } else if shown {
            // What had it may not be drawn as it stands now.
            self.focus_the_panel(window, cx);
        } else {
            self.put_away_panel(window, cx);
            self.open(Shows::SigningIn, cx);
        }
        cx.notify();
    }
}

/// What was typed of the new server `under_way` was to, once it has come to nothing, and why, with how else to add it
/// where there's no way to sign in. `None` while it hasn't, and for one to a server the hub has.
fn not_added(under_way: &UnderWay) -> Option<(NewServer, String)> {
    let SignInTo::New(typed) = under_way.to() else { return None };
    let why = match under_way.stands() {
        Stands::Starting(_) | Stands::Waiting(_) | Stands::Finishing(_) => return None,
        Stands::NoWay(no_page) => words::add_another_way(no_page),
        Stands::Failed(why) => why.clone(),
    };
    Some((typed.clone(), why))
}
