//! A server's `…` menu, which has Remove. It's off with no hub to ask.

use gpui_kit::{AnyElement, App, Context, SharedString, Window};
use protocol::model::ToolServer;

use super::super::account_row;
use super::Drawing;
use super::kept::Kept;

/// What an item does to the page's state, for a server.
type Act = fn(&mut Kept, &ToolServer, &mut Window, &mut Context<Kept>);

pub(super) fn menu(drawing: &Drawing<'_>, server: &ToolServer, cx: &App) -> AnyElement {
    let (kept, server) = (drawing.kept.clone(), server.clone());
    let id = SharedString::from(format!("mcp-more-{}", server.id));
    let name = server.name.clone();
    account_row::menu(id, name.as_str(), !drawing.can_ask(cx), move |menu: look::Menu, _, _| {
        let item = |label: &'static str, act: Act| {
            let (kept, server) = (kept.clone(), server.clone());
            look::menu_row(label)
                .on_choose(move |window, cx| kept.update(cx, |kept, cx| act(kept, &server, window, cx)))
        };
        menu.row(item("Remove", Kept::ask_to_remove).removes())
    })
}
