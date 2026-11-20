//! The hub switcher at the top of the sidebar: a compact single-line selector above Home, Create and Search.

use std::path::Path;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::Icon;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::h_flex;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::{
    Anchor, AnyElement, App, Context, Entity, Hsla, IntoElement, ParentElement as _, SharedString, Styled as _,
    WeakEntity, Window, div, px,
};
use look::{Hand as _, IconSizing as _, Shaped as _, Typeset};
use protocol::pairing::Fingerprint;

use super::{LIST_PAD, ROW_PAD, needing_a_look, row_icon};
use crate::app::{AppView, Connection, hubs, pairing, saved_hubs, share_hub};
use crate::names::{MACHINE, THIS_MACHINE};
use crate::sides::Side;

pub(super) fn render(app: &AppView, cx: &mut Context<AppView>) -> impl IntoElement {
    let selected = app.hub.remote.as_ref().map(|hub| hub.fingerprint.clone());
    let name = app.hub.remote.as_ref().map_or(THIS_MACHINE, |hub| hub.hub_name.as_str());
    let icon = if selected.is_none() { Lucide::Laptop } else { Lucide::Server };
    let colors = look::colors(cx);
    let background_attention: usize =
        hubs::other_views(cx.entity_id(), cx).iter().map(|other| needing_a_look(other.read(cx), cx)).sum();
    let (status_a11y, trailing) = switcher_trailing(app, selected.is_some(), background_attention, cx);
    let saved = crate::settings::get(cx).hubs;
    let local_connected = if app.hub.remote.is_none() {
        app.client().is_some()
    } else {
        hubs::local_view(cx)
            .filter(|local| local.entity_id() != cx.entity_id())
            .is_some_and(|local| local.read(cx).client().is_some())
    };
    let data_dir = app.hub.paths.data_dir.clone();
    let menu_width = (Side::Left.width(cx) - LIST_PAD.size() * 2.).max(px(180.));
    let view = cx.weak_entity();
    let a11y =
        status_a11y.map_or_else(|| format!("Switch hub: {name}"), |detail| format!("Switch hub: {name}, {detail}"));
    let variant =
        ButtonCustomVariant::new(cx).color(colors.inset).hover(colors.raised_hover).active(colors.raised_pressed);
    div().w_full().mb(look::Space::XSmall).child(
        Button::new("hub-switcher")
            .accessibility_label(a11y)
            .hand()
            .custom(variant)
            .w_full()
            .h(look::ControlSize::Large.height())
            .px(ROW_PAD)
            .border(look::EDGE)
            .border_color(colors.control_edge)
            .corners(look::Corner::Control)
            .child(switcher_face(icon, name, trailing, cx))
            .dropdown_menu_with_anchor(
                Anchor::TopLeft,
                look::menu(move |menu: look::Menu, _: &mut Window, cx| {
                    hub_menu(menu.width(menu_width), selected.as_ref(), &saved, &view, &data_dir, local_connected, cx)
                }),
            ),
    )
}

fn switcher_face(icon: Lucide, name: &str, trailing: Option<AnyElement>, cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    h_flex()
        .w_full()
        .gap(look::Space::Small)
        .items_center()
        .child(row_icon(icon, colors.text_muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_left()
                .truncate()
                .typeset(look::Type::Body)
                .text_color(colors.text_muted)
                .child(name.to_string()),
        )
        .children(trailing)
        .child(Icon::new(Lucide::ChevronsUpDown).sized(look::IconSize::Regular).text_color(colors.text_faint))
}

fn switcher_trailing(
    app: &AppView,
    remote: bool,
    background_attention: usize,
    cx: &App,
) -> (Option<String>, Option<AnyElement>) {
    let colors = look::colors(cx);
    match app.connection() {
        Connection::Connecting if remote => {
            (Some("Connecting".to_string()), Some(inline_detail("Connecting…", colors.text_muted)))
        }
        Connection::Failed(failure) => {
            let label = failure.switcher_label();
            let color = if failure.needs_attention() { colors.error } else { colors.text_muted };
            (Some(label.to_string()), Some(inline_detail(label, color)))
        }
        Connection::AcrossABreak(_) => {
            (Some("Update required".to_string()), Some(inline_detail("Update required", colors.warning)))
        }
        Connection::Ready(_) | Connection::Connecting if background_attention > 0 => {
            let says = attention_words(background_attention);
            let badge = look::count(look::CountKind::NeedsALook, background_attention)
                .said("hub-switcher-needs-a-look", says.clone())
                .into_any_element();
            (Some(says), Some(badge))
        }
        Connection::Ready(_) | Connection::Connecting => (None, None),
    }
}

fn inline_detail(text: &'static str, color: Hsla) -> AnyElement {
    div().flex_none().typeset(look::Type::Detail).text_color(color).child(text).into_any_element()
}

fn hub_menu(
    menu: look::Menu,
    selected: Option<&Fingerprint>,
    saved: &[saved_hubs::SavedHub],
    view: &WeakEntity<AppView>,
    data_dir: &Path,
    local_connected: bool,
    cx: &App,
) -> look::Menu {
    let local = hub_row(Lucide::Laptop, THIS_MACHINE, selected.is_none(), note_for(None, selected.is_none(), None, cx));
    let menu = menu.row(local.on_choose(|window, cx| hubs::select(None, window, cx)));
    let menu = saved.iter().fold(menu, |menu, hub| {
        let is_selected = selected == Some(&hub.fingerprint);
        let note = note_for(Some(&hub.fingerprint), is_selected, hub.hosts.first().map(String::as_str), cx);
        let (chosen, target) = (hub.clone(), view.clone());
        menu.row(
            hub_row(Lucide::Server, hub.name.clone(), is_selected, note)
                .on_choose(move |window, cx| saved_hubs::open(chosen.clone(), target.clone(), window, cx)),
        )
    });
    let pair = format!("Pair a device with this {MACHINE}…");
    let connect_data = data_dir.to_path_buf();
    menu.separator()
        .row(look::menu_row(pair).icon(Lucide::QrCode).off(!local_connected).on_choose(share_hub::open))
        .row(
            look::menu_row("Connect another hub…")
                .icon(Lucide::Plus)
                .on_choose(move |window, cx| pairing::open(connect_data.clone(), window, cx)),
        )
}

/// A hub's row: its machine's icon, its name, and its note at the end, checked while it's the one open.
fn hub_row(
    icon: Lucide,
    name: impl Into<SharedString>,
    chosen: bool,
    note: Option<(SharedString, look::NoteKind)>,
) -> look::MenuRow {
    let row = look::menu_row(name).icon(icon).chosen(chosen);
    match note {
        Some((note, kind)) => row.note(note, kind),
        None => row,
    }
}

/// What a hub's row says at its end: how many of its workspaces need a look, while it's not the one open, or how
/// connecting to it went wrong, or else its first address.
fn note_for(
    fingerprint: Option<&Fingerprint>,
    is_selected: bool,
    first_host: Option<&str>,
    cx: &App,
) -> Option<(SharedString, look::NoteKind)> {
    if let Some(view) = hubs::view_of(fingerprint, cx).as_ref().map(|v: &Entity<AppView>| v.read(cx)) {
        if !is_selected {
            let count = needing_a_look(view, cx);
            if count > 0 {
                return Some((count.to_string().into(), look::NoteKind::Count));
            }
        }
        if let Connection::Failed(failure) = view.connection() {
            let kind = if failure.needs_attention() { look::NoteKind::Failure } else { look::NoteKind::Detail };
            return Some((failure.switcher_label().into(), kind));
        }
    }
    first_host.map(|host| (host_without_port(host).to_string().into(), look::NoteKind::Detail))
}

fn host_without_port(host: &str) -> &str {
    host.rsplit_once(':').map_or(host, |(name, _)| name)
}

fn attention_words(count: usize) -> String {
    if count == 1 {
        "1 workspace needs a look on another hub".to_string()
    } else {
        format!("{count} workspaces need a look on other hubs")
    }
}
