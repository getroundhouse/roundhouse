//! Settings as it fills the window: a sidebar with a search at its top and a page for each part of the app under a
//! heading, with a way back at its top right, and the chosen page beside it. Most pages are a list of settings, and a
//! page with more to show than a setting's row, such as Accounts, draws what's on it itself.

use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::base::ScrollBounce;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{IconName, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, RenderOnce, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _,
    Window, div, px,
};
use look::{Hand as _, Sizing as _, Typeset as _};

use client_core::records::Snapshot;
use client_core::{Client, RecordsFeed};

use crate::FindSetting;
use crate::workspace_pane::HEADER_HEIGHT;

mod accounts;
mod general;
mod notifications;
mod rows;
mod sidebar;
#[cfg(test)]
mod tests;

/// The key context Settings' own keys are bound in.
pub(crate) const KEY_CONTEXT: &str = "Settings";

/// The key that puts the cursor in the search, which the search shows at its right end.
pub(crate) const FIND_KEY: &str = "secondary-f";

/// What Settings shows of the hub, and the hub to ask for a change.
pub(crate) struct OfHub {
    /// What's shown of the hub's records, as of the last change to them: its own settings, its accounts, and what it
    /// says of its agents. Kept while the hub is out of reach, and empty until they've arrived.
    pub(crate) records: Snapshot,
    /// `None` while there's no hub to ask.
    pub(crate) asking: Option<Asking>,
}

/// The hub to ask, and client-core's copy of its records, which takes in a record a reply carries, so what's shown
/// needn't wait for the hub to say the same.
#[derive(Clone)]
pub(crate) struct Asking {
    pub(crate) client: Client,
    pub(crate) feed: RecordsFeed,
}

/// What goes back from Settings to where the person was.
type Back = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Settings, with `back` going back to where the person was. Keys reach it through `focus`. `width` is how wide the
/// sidebar it stands in for is, whether or not that's showing. `hub` is what it shows of the hub.
pub(crate) fn page(
    focus: &FocusHandle,
    width: Pixels,
    back: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    hub: OfHub,
) -> SettingsView {
    SettingsView { focus: focus.clone(), width, back: Rc::new(back), hub }
}

#[derive(IntoElement)]
pub(crate) struct SettingsView {
    focus: FocusHandle,
    width: Pixels,
    back: Back,
    hub: OfHub,
}

impl RenderOnce for SettingsView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Kept only while Settings shows, so it opens with nothing typed, on its first page.
        let search = window.use_keyed_state("settings-search", cx, InputState::new);
        let chosen = window.use_keyed_state("settings-chosen", cx, |_, _| None::<PageId>);
        let scroll = window.use_keyed_state("settings-scroll", cx, |_, _| ScrollHandle::new()).read(cx).clone();
        let sections = found(sections(&self.hub), &search.read(cx).value());
        let shown = shown(&sections, *chosen.read(cx));
        let within = Within { focus: self.focus.clone(), scroll };
        let shown_page = match shown {
            Some(page) => settings_page(page, &within, cx).into_any_element(),
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
    /// Whether the search is in what it's called or what it does.
    fn matches(&self, search: &str) -> bool {
        matches(search, &[self.name, self.does])
    }
}

/// Whether `search` is in any of what's `said`, in any case. A search of nothing is in everything.
fn matches(search: &str, said: &[&str]) -> bool {
    let search = search.trim().to_lowercase();
    said.iter().any(|said| said.to_lowercase().contains(&search))
}

/// Which page, so a page is known by what it's for rather than by what it says.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PageId {
    General,
    Notifications,
    Accounts,
}

impl PageId {
    /// What its row in the sidebar is known by.
    fn row_id(self) -> &'static str {
        match self {
            PageId::General => "settings-page-general",
            PageId::Notifications => "settings-page-notifications",
            PageId::Accounts => "settings-page-accounts",
        }
    }
}

struct Page {
    id: PageId,
    name: &'static str,
    icon: Lucide,
    body: Body,
}

/// What a page shows under its name.
enum Body {
    /// Settings, a row each, which a search narrows to those that match it.
    Settings(Vec<Setting>),
    /// What the page draws itself, which a search finds whole, by the page's name or by what it's `about`, since a
    /// setting's row can't show it.
    Own { about: &'static str, draw: Draw },
}

/// Draws what's on a page of its own, each time it's the page showing.
type Draw = Box<dyn Fn(&Within) -> AnyElement>;

/// What a page is drawn within: Settings' focus, where keys reach it, for something on the page that goes while it
/// has the focus to give it back to, and the scroll of the page under its name, to bring something on it into sight.
#[derive(Clone)]
struct Within {
    focus: FocusHandle,
    scroll: ScrollHandle,
}

impl Page {
    /// Leaves on the page what `search` finds there, and says whether that's anything.
    fn narrow(&mut self, search: &str) -> bool {
        match &mut self.body {
            Body::Settings(settings) => {
                settings.retain(|setting| setting.matches(search));
                !settings.is_empty()
            }
            Body::Own { about, .. } => matches(search, &[self.name, about]),
        }
    }
}

/// Pages under a heading.
struct Section {
    heading: &'static str,
    pages: Vec<Page>,
}

/// Every page, under its heading.
fn sections(hub: &OfHub) -> Vec<Section> {
    vec![Section { heading: "Personal", pages: vec![general::page(hub), notifications::page(), accounts::page(hub)] }]
}

/// What a search leaves: the settings that match it, on their pages, and the pages of their own it finds, under their
/// headings. A page with nothing left on it goes, and so does a heading with no page left under it.
fn found(mut sections: Vec<Section>, search: &str) -> Vec<Section> {
    for section in &mut sections {
        section.pages.retain_mut(|page| page.narrow(search));
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

/// In the strip along the top, level with the window's buttons at the other end, and muted, since it's not what
/// Settings is for. A press on it is the button's, so it doesn't move the window.
fn way_back(width: Pixels, back: Back, cx: &App) -> impl IntoElement {
    div().absolute().top_0().left_0().w(width).h(HEADER_HEIGHT).flex().items_center().justify_end().pr_3().child(
        Button::new("settings-back")
            .hand()
            .ghost()
            .sized(look::ControlSize::Regular)
            .icon(IconName::ArrowLeft)
            .label("Back")
            .text_color(look::colors(cx).text_muted)
            .on_click(move |event, window, cx| back(event, window, cx)),
    )
}

/// The page's name, large, below the strip along the top, which stays clear for moving the window, and what's on it
/// under that, which scrolls once there's more than fits. It scrolls by `within`'s handle, which a page of its own can
/// bring something into sight by, so it's made of the parts of the kit's own way, which keeps its handle to itself:
/// the kit's scrollbar over it, and its bounce at the ends, on a Mac.
fn settings_page(page: &Page, within: &Within, cx: &App) -> impl IntoElement {
    let title = div()
        .flex_none()
        .px_8()
        .pt(HEADER_HEIGHT + px(20.))
        .pb_2()
        .typeset(look::Type::PageTitle)
        .text_color(look::colors(cx).text)
        .child(page.name);
    let on_it = v_flex().flex_none().w_full().px_8().py_4();
    let on_it = match &page.body {
        Body::Settings(settings) => on_it.gap_4().children(settings.iter().map(|setting| rows::row(setting, cx))),
        Body::Own { draw, .. } => on_it.child(draw(within)),
    };
    let scrolled = div()
        .id("settings-body")
        .size_full()
        .overflow_y_scroll()
        .track_scroll(&within.scroll)
        .child(on_it)
        .test_support();
    let bounce = ScrollBounce::new("settings-bounce", &within.scroll, scrolled);
    let body = div().relative().flex_1().min_h_0().overflow_hidden().child(bounce).vertical_scrollbar(&within.scroll);
    v_flex().size_full().child(title).child(body)
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
