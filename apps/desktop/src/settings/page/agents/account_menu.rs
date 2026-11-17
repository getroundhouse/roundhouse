//! An added account's `…` menu: Renew and Disconnect while the hub holds its login, since a login lasts long enough
//! that neither is wanted often, and Connect again once it was disconnected. Renew and Connect again are off while the
//! hub doesn't say what makes a token for the account's kind, and the menu is off with no hub to ask. A login found
//! where agents ran has no menu, since Roundhouse only reads it.

use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
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
    account_row::menu(id, called, off, move |menu: PopupMenu, _, _| {
        let open = {
            let (kept, account) = (kept.clone(), account.clone());
            move |window: &mut Window, cx: &mut App| {
                kept.update(cx, |kept, cx| kept.open_for(agent, &account, window, cx));
            }
        };
        let connect = if account.held_login.is_some() { "Renew" } else { "Connect again" };
        let menu = menu
            .item(PopupMenuItem::new(connect).disabled(!makes_a_token).on_click(move |_, window, cx| open(window, cx)));
        if account.held_login.is_none() {
            return menu;
        }
        let (kept, id, name) = (kept.clone(), account.id.clone(), name.clone());
        menu.item(PopupMenuItem::new("Disconnect").on_click(move |_, window, cx| {
            kept.update(cx, |kept, cx| kept.ask_to_disconnect(id.clone(), &name, window, cx));
        }))
    })
}
