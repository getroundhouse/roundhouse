//! Settings as it fills the window: a page for each part of the app in a sidebar, with a way back at its top right,
//! and the settings on the chosen page beside it. It's the kit's settings view, which also finds a setting by name.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _};
use gpui_kit::{
    App, ClickEvent, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, StyleRefinement,
    Styled as _, Window, div, px,
};

use crate::workspace_pane::HEADER_HEIGHT;

/// The key context Settings' own keys are bound in.
pub(crate) const KEY_CONTEXT: &str = "Settings";

/// Settings, with `back` going back to where the person was. Keys reach it through `focus`.
pub(crate) fn page(
    focus: &FocusHandle,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let width = px(crate::sidebar::WIDTH);
    // The search starts below the strip along the top, where the main window's sidebar starts. The kit's sidebar
    // already puts 12px above it.
    let mut search = StyleRefinement::default();
    search.padding.top = Some((HEADER_HEIGHT - px(12.)).into());
    // The same width as the sidebar it stands in for, so nothing moves, and fixed, so the way back stays over it.
    let pages = Settings::new("settings-pages").sidebar_width(width).sidebar_size_range(width..width);
    // In the strip along the top, level with the window's buttons at the other end, and muted, since it's not what
    // Settings is for. A press on it is the button's, so it doesn't move the window.
    let way_back =
        div().absolute().top_0().left_0().w(width).h(HEADER_HEIGHT).flex().items_center().justify_end().pr_3();
    div()
        .id("settings")
        .key_context(KEY_CONTEXT)
        .track_focus(focus)
        .relative()
        .size_full()
        .child(pages.header_style(&search).page(general()))
        .child(
            way_back.child(
                Button::new("settings-back")
                    .ghost()
                    .small()
                    .icon(IconName::ArrowLeft)
                    .label("Back")
                    .text_color(cx.theme().muted_foreground)
                    .on_click(back),
            ),
        )
}

/// How the app behaves wherever it is.
fn general() -> SettingPage {
    // As tall as the workspace header, so the title sits level with the window's buttons.
    let mut header = StyleRefinement::default();
    header.size.height = Some(HEADER_HEIGHT.into());
    SettingPage::new("General").resettable(false).header_style(&header).group(
        SettingGroup::new().item(
            SettingItem::new(
                "Always expand thinking",
                SettingField::switch(
                    |cx| super::get(cx).always_expand_thinking,
                    |on, cx| super::change(cx, |settings| settings.always_expand_thinking = on),
                ),
            )
            .description("Show what the agent thinks in full, rather than as a line to open."),
        ),
    )
}
