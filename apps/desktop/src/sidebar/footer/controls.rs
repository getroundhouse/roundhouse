//! The buttons at the build line's end: the question mark, whose menu has Check for Updates and Changelog, then
//! Settings' gear, as `docs/spec/updates.md` says.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::{Anchor, App, Context, IntoElement, Styled as _, Window};
use look::{Hand as _, Sizing as _};

use crate::OpenSettings;
use crate::app::AppView;

/// Where Changelog goes, as the spec's rule for the build line's buttons says: the history of `main`, newest first,
/// since each merge to it is published as the next version.
const CHANGELOG: &str = "https://github.com/getroundhouse/roundhouse/commits/main";

/// A small, faint, borderless button with only an icon, named `name` for assistive apps.
fn faint_button(id: &'static str, icon: Lucide, name: &'static str, cx: &App) -> Button {
    Button::new(id)
        .hand()
        .ghost()
        .sized_square(look::ControlSize::Small)
        .icon(icon)
        .text_color(look::colors(cx).text_faint)
        .accessibility_label(name)
}

/// A question mark, whose menu opens upward from it, toward the sidebar's left, so it stays over the sidebar.
pub(super) fn help(cx: &App) -> impl IntoElement {
    faint_button("help", Lucide::CircleQuestionMark, "Help", cx).tooltip("Help").dropdown_menu_with_anchor(
        Anchor::BottomRight,
        |menu: PopupMenu, _: &mut Window, _| {
            let check = PopupMenuItem::new("Check for Updates")
                .icon(Lucide::RefreshCw)
                .on_click(|_, _, cx| crate::updates::check_now(cx));
            let changelog =
                PopupMenuItem::new("Changelog").icon(Lucide::ScrollText).on_click(|_, _, cx| cx.open_url(CHANGELOG));
            menu.item(check).item(changelog)
        },
    )
}

/// A gear that opens Settings, as `Cmd-,` does, which its tooltip says.
pub(super) fn settings(cx: &mut Context<AppView>) -> impl IntoElement {
    faint_button("open-settings", Lucide::Settings, "Settings", cx)
        .tooltip_with_action("Settings", &OpenSettings, None)
        .on_click(cx.listener(|t, _, window, cx| t.open_settings(window, cx)))
}
