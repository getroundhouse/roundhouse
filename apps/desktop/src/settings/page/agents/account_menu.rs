//! An added account's `…` menu: Renew and Disconnect while the hub holds its login, since a login lasts long enough
//! that neither is wanted often, and Connect again once it was disconnected. Renew and Connect again are off while the
//! hub doesn't say what makes a token for the account's kind, and the menu is off with no hub to ask. A login found
//! where agents ran has no menu, since Roundhouse only reads it.

use gpui_kit::{AnyElement, App, SharedString, Window};
use protocol::model::{Account, AgentKind};

use super::super::account_row;
use super::Drawing;

/// The menu of `account`, called `called`, whose login is for `agent`'s kind.
pub(super) fn menu(drawing: &Drawing<'_>, account: &Account, agent: AgentKind, called: &str, cx: &App) -> AnyElement {
    let makes_a_token = drawing.agents.of(agent).is_some_and(|agent| agent.login.is_some());
    let off = !drawing.kept.read(cx).can_ask();
    let (kept, account, name) = (drawing.kept.clone(), account.clone(), called.to_string());
    let id = SharedString::from(format!("account-more-{}", account.id));
    account_row::menu(id, called, off, move |menu: look::Menu, _, _| {
        let open = {
            let (kept, account, name) = (kept.clone(), account.clone(), name.clone());
            move |window: &mut Window, cx: &mut App| {
                kept.update(cx, |kept, cx| kept.open_for(agent, &account, name.clone(), window, cx));
            }
        };
        let connect = if account.held_login.is_some() { "Renew" } else { "Connect again" };
        let reserve = {
            let (kept, account) = (kept.clone(), account.clone());
            look::menu_row("Reserve usage…").on_choose(move |_, cx| {
                kept.update(cx, |kept, cx| kept.open_reserving(&account, cx));
            })
        };
        let menu = menu.row(reserve).separator();
        let menu = menu.row(look::menu_row(connect).off(!makes_a_token).on_choose(move |window, cx| open(window, cx)));
        if account.held_login.is_none() {
            return menu;
        }
        let (kept, id, name) = (kept.clone(), account.id.clone(), name.clone());
        menu.row(look::menu_row("Disconnect").on_choose(move |window, cx| {
            kept.update(cx, |kept, cx| kept.ask_to_disconnect(id.clone(), &name, window, cx));
        }))
    })
}
