//! Settings as it fills the window: a sidebar with a search at its top and a page for each part of the app under a
//! heading, with a way back at its top right, and the settings on the chosen page beside it.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{IconName, Sizable as _, v_flex};
use gpui_kit::{
    App, ClickEvent, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, RenderOnce, SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};
use look::Hand as _;

use client_core::Client;
use protocol::model::{HubSettings, HubSettingsChange};

use crate::FindSetting;
use crate::workspace_pane::HEADER_HEIGHT;

mod notifications;
mod rows;
mod sidebar;
#[cfg(test)]
mod tests;

/// How big a page's name is: the one thing in Settings larger than body text, since it says where you are.
const TITLE_SIZE: Pixels = px(24.);

/// The key context Settings' own keys are bound in.
pub(crate) const KEY_CONTEXT: &str = "Settings";

/// The key that puts the cursor in the search, which the search shows at its right end.
pub(crate) const FIND_KEY: &str = "secondary-f";

/// A hub's own settings as they are now, and the hub to ask to change them.
pub(crate) struct OfHub {
    pub(crate) settings: HubSettings,
    pub(crate) client: Client,
}

/// What goes back from Settings to where the person was.
type Back = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Settings, with `back` going back to where the person was. Keys reach it through `focus`. `width` is how wide the
/// sidebar it stands in for is, whether or not that's showing. `hub` is the settings the hub keeps, or `None` while
/// they can't be read or changed.
pub(crate) fn page(
    focus: &FocusHandle,
    width: Pixels,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    hub: Option<OfHub>,
) -> SettingsView {
    SettingsView { focus: focus.clone(), width, back: Rc::new(back), hub }
}

#[derive(IntoElement)]
pub(crate) struct SettingsView {
    focus: FocusHandle,
    width: Pixels,
    back: Back,
    hub: Option<OfHub>,
}

impl RenderOnce for SettingsView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Kept only while Settings shows, so it opens with nothing typed, on its first page.
        let search = window.use_keyed_state("settings-search", cx, InputState::new);
        let chosen = window.use_keyed_state("settings-chosen", cx, |_, _| None::<PageId>);
        let sections = found(sections(self.hub.as_ref()), &search.read(cx).value());
        let shown = shown(&sections, *chosen.read(cx));
        let shown_page = match shown {
            Some(page) => settings_page(page, cx).into_any_element(),
            None => nothing_found(cx).into_any_element(),
        };
        let shown_id = shown.map(|page| page.id);
        let sidebar = sidebar::sidebar(&search, &sections, shown_id, &chosen, self.width, window, cx);
        let find = search.clone();
        div()
            .id("settings")
            .key_context(KEY_CONTEXT)
            // With what's typed selected, as a search field does, so what's typed next takes its place.
            .on_action(move |_: &FindSetting, window, cx| {
                find.update(cx, |search, cx| {
                    search.focus(window, cx);
                    search.select_all(window, cx);
                });
            })
            .relative()
            .size_full()
            .flex()
            .child(takes_focus(&self.focus))
            .child(sidebar)
            .child(div().flex_1().min_w_0().h_full().child(shown_page))
            .child(way_back(self.width, self.back, cx))
    }
}

/// Where keys reach Settings through `focus`, which a press on the page that nothing in it takes puts there, as it
/// takes the cursor out of the search. Under all of the page, and only below the strip along the top, since taking
/// focus marks a press as taken, and a press on the strip has to reach the window to move it, leaving focus where it
/// was.
fn takes_focus(focus: &FocusHandle) -> impl IntoElement {
    div().absolute().top(HEADER_HEIGHT).left_0().right_0().bottom_0().track_focus(focus)
}

/// Whether a setting is on, or `None` for one that can't be read or changed just now, such as a hub's while the hub
/// is out of reach.
type IsOn = Box<dyn Fn(&App) -> Option<bool>>;
/// Turns a setting on or off.
type Set = Rc<dyn Fn(bool, &mut App)>;
/// What's chosen now, as its button says it.
type Chosen = Box<dyn Fn(&App) -> SharedString>;
/// Fills the menu a setting chooses from, as it opens.
type Menu = Rc<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

/// A setting, as a row on its page.
struct Setting {
    name: &'static str,
    does: &'static str,
    control: Control,
}

/// How a setting is changed.
enum Control {
    /// It's on or off.
    Switch { is_on: IsOn, set: Set },
    /// It's one of a list, from a button that says which and opens a menu of them. `id` is the button's.
    Choice { id: &'static str, chosen: Chosen, menu: Menu },
}

impl Setting {
    /// Whether the search is in what it's called or what it does, in any case.
    fn matches(&self, search: &str) -> bool {
        let search = search.trim().to_lowercase();
        [self.name, self.does].iter().any(|said| said.to_lowercase().contains(&search))
    }
}

/// Which page, so a page is known by what it's for rather than by what it says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PageId {
    General,
    Notifications,
}

impl PageId {
    /// What its row in the sidebar is known by.
    fn row_id(self) -> &'static str {
        match self {
            PageId::General => "settings-page-general",
            PageId::Notifications => "settings-page-notifications",
        }
    }
}

struct Page {
    id: PageId,
    name: &'static str,
    icon: Lucide,
    settings: Vec<Setting>,
}

/// Pages under a heading.
struct Section {
    heading: &'static str,
    pages: Vec<Page>,
}

/// Every page, under its heading.
fn sections(hub: Option<&OfHub>) -> Vec<Section> {
    vec![Section { heading: "Personal", pages: vec![general(hub), notifications::page()] }]
}

/// What a search leaves: the settings that match it, on their pages, under their headings. A page with nothing left on
/// it goes, and so does a heading with no page left under it.
fn found(mut sections: Vec<Section>, search: &str) -> Vec<Section> {
    for section in &mut sections {
        for page in &mut section.pages {
            page.settings.retain(|setting| setting.matches(search));
        }
        section.pages.retain(|page| !page.settings.is_empty());
    }
    sections.retain(|section| !section.pages.is_empty());
    sections
}

/// The page to show: the one chosen, unless the search has left nothing on it, and then the first the search has
/// left. What was chosen stays chosen, so it shows again once the search no longer hides it.
fn shown(sections: &[Section], chosen: Option<PageId>) -> Option<&Page> {
    let pages = || sections.iter().flat_map(|section| &section.pages);
    chosen.and_then(|id| pages().find(|page| page.id == id)).or_else(|| pages().next())
}

/// How the app behaves wherever it is, and what its hub does whether or not the app is open.
fn general(hub: Option<&OfHub>) -> Page {
    let always_expand_thinking = Setting {
        name: "Always expand thinking",
        does: "Show what the agent thinks in full, rather than as a line to open.",
        control: Control::Switch {
            is_on: Box::new(|cx| Some(super::get(cx).always_expand_thinking)),
            set: Rc::new(|on, cx| super::change(cx, |settings| settings.always_expand_thinking = on)),
        },
    };
    let keep_awake = Setting {
        name: "Keep computer awake while agents are working",
        does: "The display still sleeps, and closing the lid still puts the computer to sleep.",
        control: hub_switch(
            hub,
            |settings| Some(settings.keep_awake),
            |on| HubSettingsChange { keep_awake: Some(on), ..HubSettingsChange::default() },
        ),
    };
    let allow_asked = Setting {
        name: "Skip Claude's safety checks",
        does: "With full access, Claude only stops to ask about commands it flags as possibly destructive, like rm -rf on a path it can't see. This lets those run without asking.",
        control: hub_switch(
            hub,
            |settings| settings.allow_asked_in_full_access,
            |on| HubSettingsChange { allow_asked_in_full_access: Some(on), ..HubSettingsChange::default() },
        ),
    };
    let settings = vec![always_expand_thinking, keep_awake, allow_asked];
    Page { id: PageId::General, name: "General", icon: Lucide::Settings2, settings }
}

/// A switch for one of the hub's own settings: on as `says` reads them, and flipped by asking the hub for the change
/// `to` makes. It shows off and can't be flipped while there's no hub to ask, or with a hub that doesn't say.
fn hub_switch(
    hub: Option<&OfHub>,
    says: fn(&HubSettings) -> Option<bool>,
    to: fn(bool) -> HubSettingsChange,
) -> Control {
    let is_on = hub.and_then(|hub| says(&hub.settings));
    let client = hub.map(|hub| hub.client.clone());
    Control::Switch {
        is_on: Box::new(move |_| is_on),
        set: Rc::new(move |on, _| {
            let Some(client) = &client else { return };
            let change = to(on);
            // Nothing waits for the reply: the switch shows what the hub says, which it says to every client once it
            // has changed, and a hub that couldn't change it leaves the switch as it was.
            drop(client.start(move |client| client.hub_configure(change)));
        }),
    }
}

/// In the strip along the top, level with the window's buttons at the other end, and muted, since it's not what
/// Settings is for. A press on it is the button's, so it doesn't move the window.
fn way_back(width: Pixels, back: Back, cx: &App) -> impl IntoElement {
    div().absolute().top_0().left_0().w(width).h(HEADER_HEIGHT).flex().items_center().justify_end().pr_3().child(
        Button::new("settings-back")
            .hand()
            .ghost()
            .small()
            .icon(IconName::ArrowLeft)
            .label("Back")
            .text_color(look::colors(cx).text_muted)
            .on_click(move |event, window, cx| back(event, window, cx)),
    )
}

/// The page's name, large, below the strip along the top, which stays clear for moving the window, and its settings
/// under it, which scroll once there are more than fit.
fn settings_page(page: &Page, cx: &App) -> impl IntoElement {
    let title = div()
        .flex_none()
        .px_8()
        .pt(HEADER_HEIGHT + px(20.))
        .pb_2()
        .text_size(TITLE_SIZE)
        .font_weight(FontWeight::MEDIUM)
        .text_color(look::colors(cx).text)
        .child(page.name);
    let rows = page.settings.iter().map(|setting| rows::row(setting, cx));
    let rows =
        v_flex().id("settings-rows").flex_1().min_h_0().px_8().py_4().gap_4().children(rows).overflow_y_scrollbar();
    v_flex().size_full().child(title).child(rows)
}

/// What shows in place of a page when the search matches no setting.
fn nothing_found(cx: &App) -> impl IntoElement {
    div()
        .id("settings-nothing-found")
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(look::colors(cx).text_muted)
        .child("No settings match")
        .test_support()
}
