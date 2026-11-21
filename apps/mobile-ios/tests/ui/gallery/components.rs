//! The design system's handheld components, each drawn as a sheet: every way one is used, at a handheld's sizes, with
//! no app around it. The desktop's gallery draws the components every client shares, at a desk's sizes.

#![cfg(test)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::assets::IconName;
use gpui_kit::component::v_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, AnyView, App, AppContext as _, Bounds, Context, HeadlessAppContext, IntoElement, ParentElement as _,
    PlatformTextSystem, Render, Styled as _, Window, WindowBounds, WindowOptions, div, point, px, size,
};
use look::{ButtonKind, ControlSize, RowSize, Space, Type, Typeset as _};
use spec_screens::Picture;

const SPEC: &str = "components.md";
const WIDTH: f32 = 402.;

pub(super) struct Sheet {
    /// Also the image's file name, in `docs/spec/screens/phone/`.
    pub(super) name: &'static str,
    pub(super) spec: &'static str,
    /// How tall its picture is, in points.
    tall: f32,
    /// Draws it, around the menus `menus` made.
    draws: fn(&[AnyView], &App) -> AnyElement,
    /// The menus it shows, made once, since a menu is a view of its own.
    menus: fn(&mut Window, &mut App) -> Vec<AnyView>,
}

pub(super) const SHEETS: &[Sheet] = &[
    Sheet { name: "top-bars", spec: SPEC, tall: 440., draws: top_bars, menus: no_menus },
    Sheet { name: "sheet", spec: SPEC, tall: 560., draws: sheet, menus: no_menus },
    Sheet { name: "menus", spec: SPEC, tall: 540., draws: menus, menus: menus_shown },
    Sheet { name: "action-sheet", spec: SPEC, tall: 560., draws: action_sheet, menus: no_menus },
];

fn no_menus(_: &mut Window, _: &mut App) -> Vec<AnyView> {
    Vec::new()
}

fn tap(id: &'static str, icon: IconName) -> look::Button {
    look::icon_button(id, ButtonKind::Borderless, ControlSize::Large, icon)
}

/// What a part of a sheet shows, over it.
fn said(says: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px(Space::Medium)
        .pt(Space::Medium)
        .pb(Space::XSmall)
        .typeset(Type::Detail)
        .text_color(look::colors(cx).text_muted)
        .child(says)
}

/// Every way a top bar is used, each under what it shows.
fn top_bars(_: &[AnyView], cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let shown = |says: &'static str, bar: look::TopBar| v_flex().child(said(says, cx)).child(bar);
    v_flex()
        .size_full()
        .bg(colors.pane)
        .child(shown("A title", look::top_bar("Settings")))
        .child(shown("A way back", look::top_bar("Notifications").start(tap("back-1", IconName::ChevronLeft))))
        .child(shown(
            "A line under the title, and buttons at its end",
            look::top_bar("Ada’s Mac mini")
                .under("Connected")
                .start(tap("hub", IconName::Server))
                .end(tap("new", IconName::Plus)),
        ))
        .child(shown(
            "A title that opens a choice of what the screen shows",
            look::top_bar("Show PR state in the sidebar")
                .under("sidebar-pr-state")
                .start(tap("back-2", IconName::ChevronLeft))
                .menu("chats", |menu, _, _| menu)
                .end(tap("more-1", IconName::Ellipsis)),
        ))
        .child(shown(
            "A title too long for its room",
            look::top_bar("Speed up opening a diff with thousands of changed lines")
                .under("faster-large-diffs")
                .start(tap("back-3", IconName::ChevronLeft))
                .end(tap("more-2", IconName::Ellipsis)),
        ))
        .into_any_element()
}

/// A sheet over a screen, for a task: connecting a GitHub account, with a row for each way, which opens it.
fn sheet(_: &[AnyView], cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let row = |id: &'static str, words: &'static str| look::list_row(id, RowSize::Regular).child(words);
    let under = v_flex()
        .size_full()
        .bg(colors.pane)
        .text_color(colors.text)
        .typeset(Type::Body)
        .child(look::top_bar("Git").start(tap("back", IconName::ChevronLeft)))
        .child(v_flex().px(look::ROWS_INSET).pt(Space::Small).children([
            row("under-1", "ada-l"),
            row("under-2", "roundhouse-ada"),
            row("under-3", "ada-ci"),
        ]));
    let over = look::sheet().title("Connect a GitHub account").clear(px(34.)).child(
        v_flex()
            .px(look::ROWS_INSET)
            .py(look::ROWS_INSET)
            .child(row("sign-in", "Sign in with GitHub").opens())
            .child(row("token", "Use a token").opens())
            .child(row("app", "Make an app of your own").opens()),
    );
    div().relative().size_full().child(under).child(over).into_any_element()
}

/// A menu with a row that removes something, last, and one to choose from.
fn menus_shown(window: &mut Window, cx: &mut App) -> Vec<AnyView> {
    let removes = |menu: look::Menu, _: &mut Window, _: &mut App| {
        menu.row(look::menu_row("Mark as unread").icon(IconName::Mail))
            .row(look::menu_row("Rename…").icon(IconName::Pencil))
            .row(look::menu_row("Regenerate title").icon(IconName::PencilSparkles))
            .separator()
            .row(look::menu_row("Archive").icon(IconName::Archive).removes())
    };
    let chooses = |menu: look::Menu, _: &mut Window, _: &mut App| {
        menu.label("Effort")
            .row(look::menu_row("Low"))
            .row(look::menu_row("Medium"))
            .row(look::menu_row("High").chosen(true))
            .row(look::menu_row("Extra high"))
    };
    vec![look::shown_menu(window, cx, removes), look::shown_menu(window, cx, chooses)]
}

/// The menus `menus_shown` made, each under what it shows.
fn menus(menus: &[AnyView], cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let [removes, chooses] = menus else { unreachable!("two menus") };
    let shown = |says: &'static str, menu: &AnyView| {
        v_flex()
            .child(said(says, cx))
            .child(div().px(Space::Medium).w(px(MENU) + Space::Medium.size()).child(menu.clone()))
    };
    v_flex()
        .size_full()
        .bg(colors.pane)
        .text_color(colors.text)
        .child(shown("A row that removes something, last, in red", removes))
        .child(shown("Rows to choose one of, under a label, with the one chosen checked", chooses))
        .into_any_element()
}

/// How wide a menu is drawn here: a little over half the screen, as an iPhone's own are.
const MENU: f32 = 250.;

/// What archiving from a menu asks first, over the list it was chosen from.
fn action_sheet(_: &[AnyView], cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let row = |id: &'static str, words: &'static str| look::list_row(id, RowSize::Large).child(words);
    let under = v_flex()
        .size_full()
        .bg(colors.pane)
        .text_color(colors.text)
        .typeset(Type::Body)
        .child(look::top_bar("Ada’s Mac mini").under("Connected"))
        .child(v_flex().px(look::ROWS_INSET).pt(Space::Small).children([
            row("under-1", "Show PR state in the sidebar"),
            row("under-2", "Ask before deleting files"),
            row("under-3", "Read settings from TOML"),
        ]));
    let over = look::action_sheet("Archive Show PR state in the sidebar?")
        .removes("archive", "Archive", |_, _, _| {})
        .clear(px(34.));
    div().relative().size_full().child(under).child(over).into_any_element()
}

struct Drawn {
    draws: fn(&[AnyView], &App) -> AnyElement,
    make: fn(&mut Window, &mut App) -> Vec<AnyView>,
    menus: Option<Vec<AnyView>>,
}

impl Render for Drawn {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let make = self.make;
        let menus = self.menus.get_or_insert_with(|| make(window, cx));
        (self.draws)(menus, cx)
    }
}

/// The sheet's picture, of all of it.
pub(super) fn take(sheet: &Sheet, text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    look::Reach::Handheld.choose();
    let mut cx = HeadlessAppContext::with_platform(
        text_system.clone(),
        Arc::new(mobile::Assets),
        gpui_kit::platform::current_headless_renderer,
    );
    cx.update(gpui_kit::init);
    cx.update(|cx| {
        look::hold_still(client_core::fake_hub::NOW_MS, cx);
        look::apply(cx);
        look::Input::Touch.choose(cx);
    });
    let bounds = Bounds::new(point(px(0.), px(0.)), size(px(WIDTH), px(sheet.tall)));
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        focus: false,
        show: false,
        ..Default::default()
    };
    let (draws, make) = (sheet.draws, sheet.menus);
    let handle =
        cx.update(|cx| cx.open_window(options, |_, cx| cx.new(|_| Drawn { draws, make, menus: None }))).unwrap();
    let mut picture = || {
        cx.run_until_parked();
        cx.update_window(handle.into(), |_, window, cx| window.render_frame(cx)).unwrap();
        cx.run_until_parked();
        cx.capture_screenshot(handle.into()).unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    let (mut last, mut same) = (picture(), 1);
    while same < 3 && Instant::now() < deadline {
        let next = picture();
        same = if next == last { same + 1 } else { 1 };
        last = next;
    }
    Picture { image: last, settled: same >= 3 }
}
