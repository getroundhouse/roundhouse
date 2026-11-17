//! An added account's `…` menu: Make default, but on the default, which is off while its login can't be used; then
//! Renew for a token, Sign in again for a sign-in, or both ways of connecting it again once it was disconnected; then
//! Disconnect, while the hub holds a login for it. The menu is off with no hub to ask.

use gpui_kit::{AnyElement, App, Context, SharedString, Window};
use protocol::model::{GithubAccount, GithubLoginKind};

use super::super::account_row;
use super::Drawing;
use super::kept::Kept;
use super::token::Renewing;
use super::words;

/// What an item does to the page's state, for `account`, the default when `default`.
type Act = fn(&mut Kept, &GithubAccount, bool, &mut Window, &mut Context<Kept>);

pub(super) fn menu(drawing: &Drawing<'_>, account: &GithubAccount, works: bool, cx: &App) -> AnyElement {
    let name = account.user.name.to_string();
    let default = drawing.is_default(account);
    let (kept, account) = (drawing.kept.clone(), account.clone());
    let id = SharedString::from(format!("git-more-{}", account.id));
    account_row::menu(id, &name, !drawing.can_ask(cx), move |menu: look::Menu, _, _| {
        let item = |label: &'static str, act: Act| {
            let (kept, account) = (kept.clone(), account.clone());
            look::menu_row(label).on_choose(move |window, cx| {
                kept.update(cx, |kept, cx| act(kept, &account, default, window, cx));
            })
        };
        let menu = if default { menu } else { menu.row(item("Make default", make_default).off(!works)) };
        let menu = match account.held_login.map(|held| held.how) {
            Some(GithubLoginKind::SignIn) => menu.row(item("Sign in again", sign_in_again)),
            // An app's installation has a menu of its own, in `app_menu`, so one here is from a hub whose record says
            // it's no installation, and a token can renew it, as it can one connected a way this app doesn't know.
            Some(GithubLoginKind::Token | GithubLoginKind::App | GithubLoginKind::Unknown) => {
                menu.row(item("Renew", renew))
            }
            None => menu.row(item("Connect again", renew)).row(item("Sign in again", sign_in_again)),
        };
        if account.held_login.is_none() {
            return menu;
        }
        menu.row(item("Disconnect", disconnect))
    })
}

fn make_default(kept: &mut Kept, account: &GithubAccount, _: bool, _: &mut Window, cx: &mut Context<Kept>) {
    kept.make_default(Some(account.id.clone()), cx);
}

/// Opens the panel for a token in place of the account's login, under its row.
fn renew(kept: &mut Kept, account: &GithubAccount, _: bool, window: &mut Window, cx: &mut Context<Kept>) {
    kept.open_token(Some(Renewing::of(account)), window, cx);
}

/// Starts a sign-in, which renews the account's login once it's approved as the account's user.
fn sign_in_again(kept: &mut Kept, account: &GithubAccount, _: bool, _: &mut Window, cx: &mut Context<Kept>) {
    kept.sign_in(Some(account.user.name.to_string()), cx);
}

fn disconnect(kept: &mut Kept, account: &GithubAccount, default: bool, window: &mut Window, cx: &mut Context<Kept>) {
    let says = words::disconnect_says(default);
    kept.ask_to_disconnect(account.id.clone(), account.user.name.as_str(), says, window, cx);
}
