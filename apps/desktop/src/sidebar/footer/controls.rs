//! The buttons at the build line's end: the question mark, whose menu has Check for Updates and Changelog, then
//! Settings' gear, as `docs/spec/updates.md` says.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::{Anchor, Context, IntoElement, Window};

use crate::OpenSettings;
use crate::app::AppView;

/// Where Changelog goes, as the spec's rule for the build line's buttons says: the history of `main`, newest first,
/// since each merge to it is published as the next version.
const CHANGELOG: &str = "https://github.com/getroundhouse/roundhouse/commits/main";

/// A small, faint, borderless button with only an icon, named `name` for assistive apps.
fn faint_button(id: &'static str, icon: Lucide, name: &'static str) -> look::Button {
    look::icon_button(id, look::ButtonKind::Faint, look::ControlSize::Small, icon).accessibility_label(name)
}

/// A question mark, whose menu opens upward from it, toward the sidebar's left, so it stays over the sidebar.
pub(super) fn help() -> impl IntoElement {
    faint_button("help", Lucide::CircleQuestionMark, "Help").tooltip("Help").menu(
        Anchor::BottomRight,
        |menu: look::Menu, _: &mut Window, _| {
            let check = look::menu_row("Check for Updates")
                .icon(Lucide::RefreshCw)
                .on_choose(|_, cx| crate::updates::check_now(cx));
            let changelog =
                look::menu_row("Changelog").icon(Lucide::ScrollText).on_choose(|_, cx| cx.open_url(CHANGELOG));
            menu.row(check).row(changelog)
        },
    )
}

/// A gear that opens Settings, as `Cmd-,` does, which its tooltip says.
pub(super) fn settings(cx: &mut Context<AppView>) -> impl IntoElement {
    faint_button("open-settings", Lucide::Settings, "Settings")
        .tooltip_with_action("Settings", &OpenSettings, None)
        .on_click(cx.listener(|t, _, window, cx| t.open_settings(window, cx)))
}
