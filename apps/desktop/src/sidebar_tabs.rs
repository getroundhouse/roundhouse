//! The two tabs atop the sidebar of changes, under the pull request's bar: all the workspace's files, first, or its
//! changes. Each workspace's pane keeps which one it shows, since that starts from whether the workspace changes
//! anything.

use std::rc::Rc;

use gpui_kit::base::{Tab, Tabs};
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, IntoElement as _, ParentElement as _, Pixels, SharedString, Styled as _, Window, div, px,
};
use look::{Hand as _, LineText};

/// As tall as the chat tabs, so the two rows of tabs line up across the window.
pub(crate) const HEIGHT: Pixels = px(40.);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarTab {
    /// Every file in the worktree.
    AllFiles,
    /// What the workspace changes against its base.
    Changes,
}

/// What reading a workspace's changes last came to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Read {
    /// They haven't been read, or not since a tab was clicked.
    #[default]
    NotYet,
    /// They couldn't be read, and never have been.
    Failed,
    Nothing,
    Something,
}

/// Whether the hub can list a worktree's files, which one older than the app may not.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Hub {
    #[default]
    ListsFiles,
    TooOld,
}

/// Which tab a workspace's sidebar of changes shows. A workspace that changes nothing shows All files, since an empty
/// list of changes shows nothing, and one that changes something shows Changes. A tab that's clicked shows instead,
/// until the workspace's first change, which is mostly an agent's, at the end of a turn, and shows Changes, since
/// that's when there's something to review.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TabShown {
    clicked: Option<SidebarTab>,
    read: Read,
    hub: Hub,
}

impl TabShown {
    /// `None` until the changes are read or a tab is clicked, so neither tab is marked and then replaced. Changes
    /// stands in where All files would say nothing: for changes that can't be read, and on a hub that can't list
    /// files.
    pub(crate) fn tab(self) -> Option<SidebarTab> {
        let by_changes = match (self.read, self.hub) {
            (Read::NotYet, _) => None,
            (Read::Nothing, Hub::ListsFiles) => Some(SidebarTab::AllFiles),
            (Read::Nothing, Hub::TooOld) | (Read::Failed | Read::Something, _) => Some(SidebarTab::Changes),
        };
        self.clicked.or(by_changes)
    }

    /// Shows the tab clicked. What its click reads isn't taken for the workspace's first change, which would show
    /// Changes in place of the tab just clicked.
    pub(crate) fn click(&mut self, tab: SidebarTab) {
        self.clicked = Some(tab);
        self.read = Read::NotYet;
    }

    /// The workspace changes this many files, as just read.
    pub(crate) fn read(&mut self, changed: usize) {
        let read = if changed == 0 { Read::Nothing } else { Read::Something };
        if (self.read, read) == (Read::Nothing, Read::Something) {
            self.clicked = None;
        }
        self.read = read;
    }

    /// The changes couldn't be read. What was read before still stands.
    pub(crate) fn read_failed(&mut self) {
        if self.read == Read::NotYet {
            self.read = Read::Failed;
        }
    }

    /// The hub said it can't list files.
    pub(crate) fn hub_cant_list_files(&mut self) {
        self.hub = Hub::TooOld;
    }

    /// The hub may be another now, which can.
    pub(crate) fn reconnected(&mut self) {
        self.hub = Hub::ListsFiles;
    }
}

/// The row of tabs, with `controls` for the tab chosen at its right end. `chosen` is `None`, and neither tab is marked,
/// until the workspace's changes are known. `changed` is how many files it changes, once that's known, which the
/// Changes tab counts.
pub(crate) fn bar(
    chosen: Option<SidebarTab>,
    changed: Option<usize>,
    controls: Option<AnyElement>,
    on_pick: impl Fn(&SidebarTab, &mut Window, &mut App) + 'static,
    line_text: &LineText,
    cx: &App,
) -> AnyElement {
    let on_pick = Rc::new(on_pick);
    let count = changed.filter(|n| *n > 0).map(|n| n.to_string());
    let tabs = [(SidebarTab::AllFiles, "All files", None), (SidebarTab::Changes, "Changes", count)].into_iter().map(
        |(tab, label, count)| {
            let on_pick = on_pick.clone();
            let on_click = move |window: &mut Window, cx: &mut App| on_pick(&tab, window, cx);
            tab_element(tab, label, count, chosen == Some(tab), on_click, line_text, cx)
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
    line_text: &LineText,
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
                .when(!selected, |d| d.text_color(muted))
                .on_click(move |_, window, cx| on_click(window, cx))
                // Its count is smaller, so it's set on the name's baseline.
                .child(line_text.body.set(div().child(label)))
                .when_some(count, |d, count| d.child(line_text.small.set(div().text_color(muted).child(count)))),
        )
        .when(selected, |d| d.child(div().absolute().left_0().right_0().bottom_0().h(px(2.)).bg(look::tab_underline())))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use SidebarTab::{AllFiles, Changes};

    /// The tab after each reading of how many files the workspace changes.
    fn after(shown: &mut TabShown, readings: &[usize]) -> Option<SidebarTab> {
        for changed in readings {
            shown.read(*changed);
        }
        shown.tab()
    }

    #[test]
    fn all_files_shows_for_no_changes_and_changes_for_some() {
        let mut shown = TabShown::default();
        assert_eq!(shown.tab(), None, "neither before they're read");

        assert_eq!(after(&mut shown, &[0]), Some(AllFiles));
        assert_eq!(after(&mut shown, &[2]), Some(Changes), "its first change");
        assert_eq!(after(&mut shown, &[0]), Some(AllFiles), "and once they've gone");
    }

    #[test]
    fn a_tab_clicked_stays_until_a_first_change() {
        let mut shown = TabShown::default();
        shown.read(1);
        shown.click(AllFiles);

        assert_eq!(after(&mut shown, &[1, 2]), Some(AllFiles), "through later changes");
        assert_eq!(after(&mut shown, &[0]), Some(AllFiles), "and their going");
        assert_eq!(after(&mut shown, &[1]), Some(Changes), "until there are changes again");
    }

    #[test]
    fn a_click_that_finds_the_first_change_keeps_its_tab() {
        let mut shown = TabShown::default();
        shown.read(0);

        shown.click(AllFiles);

        assert_eq!(after(&mut shown, &[1]), Some(AllFiles));
    }

    #[test]
    fn changes_that_cant_be_read_show_changes_until_they_can() {
        let mut shown = TabShown::default();

        shown.read_failed();

        assert_eq!(shown.tab(), Some(Changes));
        assert_eq!(after(&mut shown, &[0]), Some(AllFiles));
        shown.read_failed();
        assert_eq!(shown.tab(), Some(AllFiles), "what was read still stands");
    }

    #[test]
    fn a_hub_that_cant_list_files_shows_changes_unless_all_files_is_clicked() {
        let mut shown = TabShown::default();
        shown.read(0);

        shown.hub_cant_list_files();

        assert_eq!(shown.tab(), Some(Changes));
        shown.reconnected();
        assert_eq!(shown.tab(), Some(AllFiles), "another hub may list them");
        shown.hub_cant_list_files();
        shown.click(AllFiles);
        assert_eq!(shown.tab(), Some(AllFiles), "which says the hub can't");
    }
}
