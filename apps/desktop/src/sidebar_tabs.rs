//! The two tabs atop the sidebar of changes, under the pull request's bar: all the workspace's files, first, or its
//! changes. Changes is chosen at the start, so a workspace's changes are in view from the start, and the one chosen
//! shows in every workspace until the app quits, as the sidebar's other choices do.

use std::rc::Rc;

use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Global, IntoElement as _, ParentElement as _, Pixels, SharedString, Styled as _, Window, div, px,
    relative,
};
use look::Hand as _;

/// As tall as the chat tabs, so the two rows of tabs line up across the window.
pub(crate) const HEIGHT: Pixels = px(40.);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SidebarTab {
    /// Every file in the worktree.
    AllFiles,
    /// What the workspace changes against its base.
    #[default]
    Changes,
}

/// The tab chosen, which every workspace shares until the app quits.
struct Chosen(SidebarTab);

impl Global for Chosen {}

pub(crate) fn chosen(cx: &App) -> SidebarTab {
    cx.try_global::<Chosen>().map(|chosen| chosen.0).unwrap_or_default()
}

pub(crate) fn choose(tab: SidebarTab, cx: &mut App) {
    cx.set_global(Chosen(tab));
}

/// The row of tabs, with `controls` for the tab chosen at its right end. `changed` is how many files the workspace
/// changes, once that's known, which the Changes tab counts.
pub(crate) fn bar(
    changed: Option<usize>,
    controls: Option<AnyElement>,
    on_pick: impl Fn(&SidebarTab, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let chosen = chosen(cx);
    let on_pick = Rc::new(on_pick);
    let count = changed.filter(|n| *n > 0).map(|n| n.to_string());
    let tabs = [(SidebarTab::AllFiles, "All files", None), (SidebarTab::Changes, "Changes", count)].into_iter().map(
        |(tab, label, count)| {
            let on_pick = on_pick.clone();
            tab_element(tab, label, count, chosen == tab, move |window, cx| on_pick(&tab, window, cx), cx)
        },
    );
    h_flex()
        .relative()
        .flex_none()
        .w_full()
        .h(HEIGHT)
        .pl_1()
        .pr_2()
        .gap_2()
        // The bar's bottom edge, under the tabs rather than a border, so the line under the tab chosen can cover it,
        // as it does under the chat tabs.
        .child(div().absolute().left_0().right_0().bottom_0().h(px(1.)).bg(cx.theme().border))
        .child(Tabs::new("sidebar-tabs").flex().h_full().children(tabs))
        .child(div().flex_1())
        .children(controls)
        .into_any_element()
}

fn tab_element(
    tab: SidebarTab,
    label: &'static str,
    count: Option<String>,
    selected: bool,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let id: SharedString = match tab {
        SidebarTab::Changes => "sidebar-tab-changes".into(),
        SidebarTab::AllFiles => "sidebar-tab-all-files".into(),
    };
    div()
        .relative()
        .flex_none()
        .h_full()
        .child(
            Tab::new(id)
                .hand()
                .selected(selected)
                .accessibility_label(label)
                .h_full()
                .px_3()
                .gap_1p5()
                .text_sm()
                .line_height(relative(1.25))
                .when(!selected, |d| d.text_color(muted))
                .on_click(move |_, window, cx| on_click(window, cx))
                .child(label)
                .when_some(count, |d, count| d.child(div().text_xs().text_color(muted).child(count))),
        )
        .when(selected, |d| d.child(div().absolute().left_0().right_0().bottom_0().h(px(2.)).bg(look::tab_underline())))
        .into_any_element()
}
