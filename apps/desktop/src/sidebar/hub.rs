//! The selected machine stays visible above the workspace list.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, v_flex};
use gpui_kit::{Anchor, Context, IntoElement, ParentElement as _, Styled as _, WeakEntity, Window, div};
use look::{Hand as _, Shaped as _, Sizing as _, Typeset as _};
use protocol::pairing::Fingerprint;

use crate::app::{AppView, Connection, hubs, pairing, saved_hubs, share_hub};

pub(super) fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let selected = app.hub.remote.as_ref().map(|hub| hub.fingerprint.clone());
    let name = app.hub.remote.as_ref().map_or(local_name(), |hub| hub.hub_name.as_str());
    let status = match app.connection() {
        Connection::Ready(_) if selected.is_none() => "This device",
        Connection::Ready(_) => "Connected",
        Connection::Connecting => "Connecting…",
        Connection::Failed(_) => "Disconnected",
        Connection::AcrossABreak(_) => "Update required",
    };
    let colors = look::colors(cx);
    let text_muted = colors.text_muted;
    let background = colors.inset;
    let border = colors.control_edge;
    let saved = crate::settings::get(cx).hubs;
    let connected = app.client().is_some();
    let view = cx.weak_entity();
    div().px(super::LIST_PAD).pb(super::LIST_PAD).child(
        Button::new("hub-switcher")
            .accessibility_label(format!("Switch hub: {name}, {status}"))
            .hand()
            .ghost()
            .sized(look::ControlSize::Large)
            .w_full()
            .h_auto()
            .py(look::Space::Small)
            .bg(background)
            .border_1()
            .border_color(border)
            .corners(look::Corner::Control)
            .icon(Lucide::Monitor)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .items_start()
                    .child(div().w_full().text_left().truncate().typeset(look::Type::Body).child(name.to_string()))
                    .child(div().typeset(look::Type::Detail).text_color(text_muted).child(status)),
            )
            .child(Icon::new(Lucide::ChevronDown))
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu: PopupMenu, _: &mut Window, _| {
                hub_menu(menu, selected.as_ref(), &saved, &view, connected)
            }),
    )
}

fn hub_menu(
    menu: PopupMenu,
    selected: Option<&Fingerprint>,
    saved: &[saved_hubs::SavedHub],
    view: &WeakEntity<AppView>,
    connected: bool,
) -> PopupMenu {
    let menu = menu.label("Switch hub").item(
        PopupMenuItem::new(format!("{} · This device", local_name()))
            .checked(selected.is_none())
            .on_click(|_, window, cx| hubs::select(None, window, cx)),
    );
    let menu = saved.iter().fold(menu, |menu, saved| {
        let saved = saved.clone();
        let view = view.clone();
        menu.item(
            PopupMenuItem::new(saved.name.clone())
                .checked(selected == Some(&saved.fingerprint))
                .on_click(move |_, window, cx| saved_hubs::open(saved.clone(), view.clone(), window, cx)),
        )
    });
    let menu = menu.separator();
    let menu = if selected.is_none() {
        let view = view.clone();
        menu.item(
            PopupMenuItem::new("Create pairing link…")
                .disabled(!connected)
                .on_click(move |_, window, cx| share_hub::open(view.clone(), window, cx)),
        )
    } else {
        menu
    };
    let view = view.clone();
    menu.item(
        PopupMenuItem::new("Connect another hub…")
            .on_click(move |_, window, cx| pairing::open(view.clone(), window, cx)),
    )
}

#[cfg(target_os = "macos")]
fn local_name() -> &'static str {
    "This Mac"
}

#[cfg(not(target_os = "macos"))]
fn local_name() -> &'static str {
    "This PC"
}
