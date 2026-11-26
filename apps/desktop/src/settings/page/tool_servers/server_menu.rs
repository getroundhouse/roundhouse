//! A server's `…` menu: Sign in again for one that's signed in to, Edit, then Remove. It's off with no hub to ask. Sign
//! in again and Edit are off while a panel is open, as Add server is, since either opens a panel in its place, so
//! nothing typed in one goes. A server that knows the person in a way only a newer hub has can't be edited here.

use gpui_kit::{AnyElement, App, Context, SharedString, Window};
use protocol::model::{ToolServer, ToolServerAuth};

use super::super::account_row;
use super::Drawing;
use super::kept::Kept;

/// What an item does to the page's state, for a server.
type Act = fn(&mut Kept, &ToolServer, &mut Window, &mut Context<Kept>);

pub(super) fn menu(drawing: &Drawing<'_>, server: &ToolServer, cx: &App) -> AnyElement {
    let (kept, server) = (drawing.kept.clone(), server.clone());
    let id = SharedString::from(format!("mcp-more-{}", server.id));
    let name = server.name.clone();
    let panel_open = drawing.kept.read(cx).panel.is_some();
    account_row::menu(id, name.as_str(), !drawing.can_ask(cx), move |menu: look::Menu, _, _| {
        let item = |label: &'static str, act: Act| {
            let (kept, server) = (kept.clone(), server.clone());
            look::menu_row(label)
                .on_choose(move |window, cx| kept.update(cx, |kept, cx| act(kept, &server, window, cx)))
        };
        let menu = match server.auth {
            ToolServerAuth::SignIn { .. } => menu.row(item("Sign in again", Kept::sign_in_again).off(panel_open)),
            ToolServerAuth::Key { .. } | ToolServerAuth::Neither | ToolServerAuth::Unknown => menu,
        };
        let menu = match server.auth {
            ToolServerAuth::SignIn { .. } | ToolServerAuth::Key { .. } | ToolServerAuth::Neither => {
                menu.row(item("Edit", Kept::open_edit).off(panel_open)).separator()
            }
            ToolServerAuth::Unknown => menu,
        };
        menu.row(item("Remove", Kept::ask_to_remove).removes())
    })
}
