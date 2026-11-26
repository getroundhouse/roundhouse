//! Added, the servers a person gave the hub: Add server by its heading, the panel for adding one under that, and a row
//! for each server, with its menu, and the panel for editing it under it.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::{AnyElement, App, Div, IntoElement as _, ParentElement as _, SharedString, Styled as _, div};
use protocol::model::ToolServer;

use super::super::account_row::{self, Detail, Picture, Row};
use super::super::lists::{heading, list as rows_of, said_in_the_list};
use super::kept::{Kept, Panel};
use super::servers_page::COLUMNS;
use super::{Drawing, panels, server_menu, words};

pub(super) fn list(drawing: &Drawing<'_>, cx: &App) -> Div {
    let kept = drawing.kept.read(cx);
    let opening = drawing.kept.clone();
    let add = look::button("mcp-add", look::ButtonKind::Outlined, look::ControlSize::Regular, "Add server")
        .icon(IconName::Plus)
        .off(!drawing.can_ask(cx) || kept.panel.is_some())
        .on_click(move |_, window, cx| opening.update(cx, |kept: &mut Kept, cx| kept.open_add(window, cx)));
    let servers = drawing.records.tool_servers();
    // One whose server has gone meanwhile is here too, with what's typed in it.
    let under = kept.panel_under(cx).filter(|id| servers.iter().any(|server| server.id == **id));
    let panel = kept.panel.as_ref().filter(|_| under.is_none());
    let panel = panel.map(|panel| div().py(look::Space::Large).child(panels::draw(drawing, panel, cx)));
    let none = (drawing.arrived() && servers.is_empty() && panel.is_none())
        .then(|| said_in_the_list("mcp-none-added", words::NONE_ADDED.into(), cx));
    let rows = servers.iter().map(|server| {
        let panel = kept.panel.as_ref().filter(|_| under == Some(&server.id));
        server_row(drawing, server, panel, cx)
    });
    let list = rows_of(cx).children(panel).children(none).children(rows);
    v_flex().child(heading("Added", None, words::ADDED_SAYS, Some(add.into_any_element()), cx)).child(list)
}

/// `server`'s row, with why the hub couldn't do what was asked of it under it, and the `panel` it's edited in.
fn server_row(drawing: &Drawing<'_>, server: &ToolServer, panel: Option<&Panel>, cx: &App) -> AnyElement {
    let id = SharedString::from(format!("mcp-server-{}", server.id));
    let failed = drawing.kept.read(cx).failed(&server.id).map(|said| account_row::failed_line(&id, said, cx));
    let panel = panel.map(|panel| {
        div().pt(look::Space::Small).pb(look::Space::Large).child(panels::draw(drawing, panel, cx)).into_any_element()
    });
    let shown = Row {
        id,
        picture: Picture::Mark(Lucide::Plug),
        name: server.name.to_string(),
        dim: false,
        default: false,
        how: Some(server.url.host().to_string().into()),
        detail: words::known_by(&server.auth).map(|(said, tone)| Detail::Said(said.to_string(), tone)),
        menu: Some(server_menu::menu(drawing, server, cx)),
        card: None,
        under: failed.into_iter().chain(panel).collect(),
    };
    account_row::row(shown, COLUMNS, cx)
}
