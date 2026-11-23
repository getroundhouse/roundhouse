//! A permission's control in its row: how it stands, with a sign before what's wrong, then the button that fixes it,
//! if anything can. Nothing until the app has first read how they stand, which it does as the window opens.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
};
use look::IconSizing as _;

use crate::permissions::{self, Permission, Standing};

pub(super) fn control(permission: Permission, cx: &App) -> AnyElement {
    let Some(standing) = permissions::standing(permission, cx) else { return gpui_kit::Empty.into_any_element() };
    h_flex()
        .items_center()
        .gap(look::Space::Large)
        .child(said(permission, standing, cx))
        .children(fix(permission, standing))
        .into_any_element()
}

/// How it stands, in words, which a screen reader says as one label, since it can't see the colour. Tests find it by
/// `settings-permission-<id>-says`.
fn said(permission: Permission, standing: Standing, cx: &App) -> impl IntoElement + use<> {
    let colors = look::colors(cx);
    let (icon, words, color) = match standing {
        Standing::Allowed => (Some(Lucide::Check), "Allowed", colors.text_muted),
        Standing::NotAsked => (None, "Not asked yet", colors.text_faint),
        Standing::Off => (Some(Lucide::TriangleAlert), "Off", colors.warning),
        Standing::Unknown => (None, "Can't tell", colors.text_faint),
    };
    h_flex()
        .id(SharedString::from(format!("settings-permission-{}-says", permission.id())))
        .role(Role::Label)
        .aria_label(SharedString::from(words))
        .flex_none()
        .gap(look::Space::Small)
        .text_color(color)
        .children(icon.map(|icon| Icon::new(icon).sized(look::IconSize::Regular)))
        .child(words)
        .test_support()
}

/// The button that fixes it: Allow…, where the system hasn't asked, which has it ask, and Open System Settings, where
/// only that changes it. None for one that's allowed, or that the system won't say of.
fn fix(permission: Permission, standing: Standing) -> Option<look::Button> {
    let words = match standing {
        Standing::NotAsked => "Allow…",
        Standing::Off => "Open System Settings",
        Standing::Allowed | Standing::Unknown => return None,
    };
    let id = format!("settings-permission-{}", permission.id());
    let button = look::button(id, look::ButtonKind::Outlined, look::ControlSize::Regular, words)
        .accessibility_label(format!("{words} {}", permission.name()))
        .on_click(move |_, _, cx| permissions::fix(permission, cx));
    Some(button)
}
