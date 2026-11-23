//! Pasting a token, for a new GitHub account or in place of an account's own login. The token is handed to the hub and
//! kept nowhere else: not on disk, not in a log and not in the app's settings. Its field is made anew once the hub has
//! answered, so nothing of it stays.

use gpui_kit::{App, Context, Task, Window};
use protocol::model::{GithubAccount, GithubAccountId, GithubToken};

use super::super::field::Field;
use super::kept::{Kept, Shows};
use super::words;

/// The panel for pasting a token.
pub(super) struct TokenPanel {
    /// The account it renews the login of, or `None` for a new account.
    pub(super) renewing: Option<Renewing>,
    /// Where the token is pasted, shown masked.
    pub(super) token: Field,
    /// What the hub said when it last refused, for the person to read.
    pub(super) refused: Option<String>,
    /// The wait for what the hub answers, while it hasn't. It goes with the panel, so an answer to a panel that was
    /// closed, or replaced by another, lands on none.
    connecting: Option<Task<()>>,
}

/// The account a token panel renews the login of.
pub(super) struct Renewing {
    pub(super) id: GithubAccountId,
    /// Its username, which the panel says.
    pub(super) name: String,
    /// Whether the hub holds a login for it now, so this renews it, rather than connects it again.
    pub(super) held: bool,
}

impl Renewing {
    /// Renewing `account`, whose user is called `name`.
    pub(super) fn of(account: &GithubAccount, name: &str) -> Renewing {
        Renewing { id: account.id.clone(), name: name.to_string(), held: account.held_login.is_some() }
    }
}

impl Kept {
    /// Opens the panel for pasting a token, for a new account, or for `renewing`'s login, with the cursor in its field.
    pub(super) fn open_token(&mut self, renewing: Option<Renewing>, window: &mut Window, cx: &mut Context<Self>) {
        let token = Field::token(window, cx, Kept::connect);
        self.open(Shows::Token(TokenPanel { renewing, token, refused: None, connecting: None }), cx);
    }

    fn token_panel(&self) -> Option<&TokenPanel> {
        let Some(Shows::Token(token)) = self.panel.as_ref().map(|panel| &panel.shows) else { return None };
        Some(token)
    }

    fn token_panel_mut(&mut self) -> Option<&mut TokenPanel> {
        let Some(Shows::Token(token)) = self.panel.as_mut().map(|panel| &mut panel.shows) else { return None };
        Some(token)
    }

    /// Whether the token panel has a token, and isn't waiting on the hub, with a hub to ask.
    pub(super) fn can_connect(&self, cx: &App) -> bool {
        let Some(panel) = self.token_panel() else { return false };
        self.can_ask() && panel.connecting.is_none() && panel.token.filled(cx)
    }

    /// Hands the hub the token pasted, for a new account or for the one the panel renews.
    pub(super) fn connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_connect(cx) {
            return;
        }
        let (Some(asking), Some(panel)) = (self.asking.clone(), self.token_panel_mut()) else { return };
        let token = GithubToken::new(panel.token.value(cx));
        let account = panel.renewing.as_ref().map(|renewing| renewing.id.clone());
        let connecting = asking.client.start(move |client| client.github_account_connect(token, account));
        panel.connecting = Some(cx.spawn_in(window, async move |this, cx| {
            let answer = connecting.recv().await;
            let _ = this.update_in(cx, |this, window, cx| this.answered(answer, window, cx));
        }));
        cx.notify();
    }

    /// Closes the panel once the hub has connected the token, and otherwise says why it couldn't, with the token's
    /// field made anew, so what was pasted is gone. The panel is the one that asked, since the wait goes with it.
    fn answered(&mut self, answer: client_core::Result<GithubAccount>, window: &mut Window, cx: &mut Context<Self>) {
        match answer {
            Ok(_) => self.put_away_panel(window, cx),
            Err(e) => {
                let token = Field::token(window, cx, Kept::connect);
                let Some(panel) = self.token_panel_mut() else { return };
                (panel.token, panel.refused, panel.connecting) = (token, Some(words::couldnt("connect it", &e)), None);
                cx.notify();
            }
        }
    }
}
