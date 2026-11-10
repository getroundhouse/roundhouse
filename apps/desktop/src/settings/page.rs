//! Settings as it fills the window: a page for each part of the app in a sidebar, with a way back at its top right,
//! and the settings on the chosen page beside it. It's the kit's settings view, which also finds a setting by name.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use look::{Hand as _, Off as _};
use std::rc::Rc;

use client_core::Client;
use gpui_kit::component::setting::{SettingGroup, SettingItem, SettingPage, Settings};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    App, ClickEvent, FocusHandle, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StyleRefinement, Styled as _, Window, div, px,
};

use protocol::model::{HubSettings, HubSettingsChange};

use crate::workspace_pane::HEADER_HEIGHT;

/// How big a page's name is: the one thing in Settings larger than body text, since it says where you are.
const TITLE_SIZE: Pixels = px(24.);

/// The key context Settings' own keys are bound in.
pub(crate) const KEY_CONTEXT: &str = "Settings";

/// A hub's own settings as they are now, and the hub to ask to change them.
pub(crate) struct OfHub {
    pub(crate) settings: HubSettings,
    pub(crate) client: Client,
}

/// Settings, with `back` going back to where the person was. Keys reach it through `focus`. `width` is how wide the
/// sidebar it stands in for is, whether or not that's showing. `hub` is the settings the hub keeps, or `None` while
/// they can't be read or changed.
pub(crate) fn page(
    focus: &FocusHandle,
    width: Pixels,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    hub: Option<OfHub>,
    cx: &App,
) -> impl IntoElement {
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
        .child(pages.header_style(&search).page(general(hub, cx)))
        .child(
            way_back.child(
                Button::new("settings-back")
                    .hand()
                    .ghost()
                    .small()
                    .icon(IconName::ArrowLeft)
                    .label("Back")
                    .text_color(cx.theme().muted_foreground)
                    .on_click(back),
            ),
        )
}

/// How the app behaves wherever it is, and what its hub does whether or not the app is open.
fn general(hub: Option<OfHub>, cx: &App) -> SettingPage {
    let always_expand_thinking = switch(
        "Always expand thinking",
        "Show what the agent thinks in full, rather than as a line to open.",
        |cx| Some(super::get(cx).always_expand_thinking),
        |on, cx| super::change(cx, |settings| settings.always_expand_thinking = on),
    );
    let hub_says = hub.as_ref().map(|hub| hub.settings.keep_awake);
    let keep_awake = switch(
        "Keep computer awake while agents are working",
        "The display still sleeps, and closing the lid still puts the computer to sleep.",
        move |_| hub_says,
        move |on, _| {
            let Some(hub) = &hub else { return };
            let change = HubSettingsChange { keep_awake: Some(on) };
            // Nothing waits for the reply: the switch shows what the hub says, which it says to every client once
            // it has changed, and a hub that couldn't change it leaves the switch as it was.
            drop(hub.client.start(move |client| client.hub_configure(change)));
        },
    );
    // Inset as far again as the kit insets the page, so the rows and their lines start under the title.
    let rows = SettingGroup::new().px_4().item(always_expand_thinking).item(keep_awake);
    SettingPage::new("General").resettable(false).header_style(&title_style(cx)).group(rows)
}

/// The page's name, large and on the page itself, below the strip along the top, which stays clear for moving the
/// window. The kit draws it as a bar with a line under it, which this takes away.
fn title_style(cx: &App) -> StyleRefinement {
    let mut title = div()
        .px_8()
        .pt(HEADER_HEIGHT + px(20.))
        .pb_2()
        .border_b_0()
        .text_size(TITLE_SIZE)
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().foreground);
    title.style().clone()
}

/// A setting that's on or off: what it's called and what it does on the left, its switch on the right, and a line
/// under it that sets it apart from the next. The kit's own row has no line, so this draws the whole row. It's found
/// by what it's called and what it does, as the kit's is. `is_on` says `None` for one that can't be read or changed
/// just now, such as a hub's while the hub is out of reach, which shows off and can't be flipped.
fn switch(
    name: &'static str,
    does: &'static str,
    is_on: impl Fn(&App) -> Option<bool> + 'static,
    set: impl Fn(bool, &mut App) + 'static,
) -> SettingItem {
    let set = Rc::new(set);
    SettingItem::render(move |_, _, cx| {
        let on = is_on(cx);
        let said = v_flex()
            .flex_1()
            .gap_1()
            .child(div().font_weight(FontWeight::MEDIUM).child(name))
            .child(div().text_color(cx.theme().muted_foreground).child(does));
        let set = set.clone();
        let flip = Switch::new(name)
            .accessibility_label(name)
            .checked(on.unwrap_or(false))
            .off_while(on.is_none())
            .on_click(move |on: &bool, _, cx: &mut App| set(*on, cx));
        h_flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap_6()
            .pt_1()
            .pb_5()
            .border_b_1()
            .border_color(cx.theme().border)
            .text_sm()
            .child(said)
            .child(flip)
    })
    .keywords([name, does])
}
