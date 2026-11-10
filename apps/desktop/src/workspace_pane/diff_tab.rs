//! The sidebar of changes on the right, and the workspace's one diff tab, which shows the file last clicked in it.
//! Clicking another file shows that one in the same tab, so reviewing a branch never leaves a trail of tabs.

use gpui_kit::base::Tab;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Global, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled, Window, div, px, relative,
};

use super::view::{TAB_GROUP, TAB_WIDTH};
use super::{Diff, DiffTab, PaneEvent, Showing, WorkspacePane};
use crate::changes::{Changes, ChangesEvent};
use crate::file_diff::{FileDiff, FileDiffEvent};
use crate::review::{Review, ReviewEvent};

/// Whether the sidebar of changes shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum Sidebar {
    /// It starts open, so a workspace's changes and where its pull request stands are in view from the start.
    #[default]
    Open,
    Hidden,
}

/// The sidebar chosen, which every workspace shares until the app quits.
struct Chosen(Sidebar);

impl Global for Chosen {}

fn sidebar(cx: &App) -> Sidebar {
    cx.try_global::<Chosen>().map(|chosen| chosen.0).unwrap_or_default()
}

impl WorkspacePane {
    /// The sidebar of changes shows, in every workspace.
    pub fn changes_open(&self, cx: &App) -> bool {
        sidebar(cx) == Sidebar::Open
    }

    /// Shows or hides the sidebar of changes, in every workspace, loading the diff again as it opens.
    pub fn toggle_changes(&mut self, cx: &mut Context<Self>) {
        let next = match sidebar(cx) {
            Sidebar::Open => Sidebar::Hidden,
            Sidebar::Hidden => Sidebar::Open,
        };
        cx.set_global(Chosen(next));
        self.load_changes(cx);
        cx.notify();
    }

    /// Loads the diff, or loads it again, if the sidebar of changes or the diff tab shows it.
    pub(super) fn load_changes(&mut self, cx: &mut Context<Self>) {
        let open = self.changes_open(cx);
        match &self.diff {
            Some(diff) if open || diff.tab.is_some() => diff.review.update(cx, Review::refresh),
            None if open => self.make_diff(cx),
            Some(_) | None => {}
        }
    }

    /// The file the diff tab shows, if it's open.
    pub fn diff_tab(&self) -> Option<SharedString> {
        self.diff.as_ref()?.tab.as_ref().map(|tab| tab.path.clone())
    }

    /// Shows a file in the diff tab, opening the tab if it isn't, and brings it to the front.
    pub fn open_diff_tab(&mut self, path: SharedString, cx: &mut Context<Self>) {
        self.set_diff_tab(Some(path), cx);
        self.bring_to_front(Showing::Diff, cx);
    }

    pub fn show_diff_tab(&mut self, cx: &mut Context<Self>) {
        self.bring_to_front(Showing::Diff, cx);
    }

    /// Closes the diff tab, and shows the chat it covered.
    pub fn close_diff_tab(&mut self, cx: &mut Context<Self>) {
        self.set_diff_tab(None, cx);
        self.bring_to_front(Showing::Chat, cx);
    }

    /// Opens the diff tab on a file, in place of the one it showed, or closes it. It's the only way the tab's file
    /// changes, so the tab's view and the sidebar's marked row always say the same file.
    fn set_diff_tab(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        let Some(diff) = &mut self.diff else { return };
        let in_front = diff.tab.as_ref().is_some_and(|tab| tab.in_front);
        diff.tab = path.clone().map(|path| DiffTab { path, in_front });
        diff.file.update(cx, |f, cx| f.show(path.clone(), cx));
        diff.changes.update(cx, |c, cx| c.set_showing(path, cx));
    }

    /// Adds a comment to what's typed in the active chat's composer, on a line of its own, and shows the chat with the
    /// composer ready to say more.
    fn add_to_composer(&mut self, text: String, cx: &mut Context<Self>) {
        self.show_chat(cx);
        self.with_window(cx, move |t, window, cx| {
            let typed = t.composer.read(cx).text().to_string();
            let joined = if typed.trim().is_empty() { text } else { format!("{}\n{text}", typed.trim_end()) };
            let end = joined.len();
            t.composer.update(cx, |c, cx| {
                c.set_value(joined, window, cx);
                c.set_selected_range(end..end, cx);
            });
        });
    }

    /// Closes whichever tab is showing, as `Cmd-W` does.
    pub fn close_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.showing() == Showing::Diff {
            self.close_diff_tab(cx);
        } else if let Some(id) = self.active.clone() {
            self.close_chat(id, window, cx);
        }
    }

    /// Makes the diff, and its two views, which start loading it.
    fn make_diff(&mut self, cx: &mut Context<Self>) {
        let (client, records, id) = (self.client.clone(), self.records.clone(), self.workspace.id.clone());
        let comments = self.comments.clone();
        let review = cx.new(|cx| Review::new(client, records, id, comments, cx));
        let file = cx.new(|cx| FileDiff::new(review.clone(), cx));
        let changes = cx.new(|cx| Changes::new(review.clone(), cx));
        let review_events = cx.subscribe(&review, |this, _, event: &ReviewEvent, cx| match event {
            ReviewEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            ReviewEvent::SendToAgent(prompt) => {
                if let Some(transcript) = this.transcript() {
                    transcript.update(cx, |t, cx| t.send_prompt(prompt.clone(), cx));
                }
                // So the agent can be seen taking them up.
                this.show_chat(cx);
            }
            ReviewEvent::Loaded => {}
        });
        let file_events = cx.subscribe(&file, |this, _, event: &FileDiffEvent, cx| match event {
            FileDiffEvent::AddToChat(text) => this.add_to_composer(text.clone(), cx),
        });
        let changes_events = cx.subscribe(&changes, |this, _, event: &ChangesEvent, cx| match event {
            ChangesEvent::Open(path) => this.open_diff_tab(path.clone(), cx),
        });
        self.subscriptions.extend([review_events, file_events, changes_events]);
        self.diff = Some(Diff { review, file, changes, tab: None });
    }

    /// The sidebar of changes, beside the chat and the diff tab, while it's open, with the pull request's bar at its
    /// top, level with the workspace's header.
    pub(super) fn changes_sidebar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let changes = self.diff.as_ref().filter(|_| self.changes_open(cx))?.changes.clone();
        let act = cx.listener(|t, action, _, cx| t.act(*action, cx));
        let bar = crate::pull_request::bar(&self.pull_request, self.busy, act, cx);
        let theme = cx.theme();
        let column = v_flex().flex_none().w(crate::changes::WIDTH).h_full().border_l_1().border_color(theme.border);
        Some(column.bg(theme.sidebar).child(bar).child(div().flex_1().min_h_0().child(changes)).into_any_element())
    }

    /// The diff tab, before the chats, named for the file it shows. Its name is in italics, because it's replaced rather
    /// than added to. It's built as a chat's tab is, with the same width, underline and close button, so the row reads
    /// as one row of tabs.
    pub(super) fn diff_tab_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.diff_tab()?;
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        let selected = self.showing() == Showing::Diff;
        let muted = cx.theme().muted_foreground;
        let close =
            Button::new("close-diff-tab").ghost().xsmall().icon(IconName::Close).tooltip("Close diff tab").on_click(
                cx.listener(|t, _, _, cx| {
                    // The tab under it would take the click too, and show the diff that's closing.
                    cx.stop_propagation();
                    t.close_diff_tab(cx);
                }),
            );
        let tab = div()
            .id("diff-tab")
            .group(TAB_GROUP)
            .relative()
            .flex_none()
            .w(TAB_WIDTH)
            .h_full()
            .child(
                Tab::new("show-diff-tab")
                    .selected(selected)
                    .accessibility_label(format!("Diff of {path}"))
                    // Two files can share a name, so the whole path is a hover away.
                    .tooltip(move |window, cx| Tooltip::new(path.clone()).build(window, cx))
                    .size_full()
                    .justify_start()
                    .gap_2()
                    .pl_3()
                    .pr_1p5()
                    .text_sm()
                    .line_height(relative(1.25))
                    .when(!selected, |d| d.text_color(muted))
                    .on_click(cx.listener(|t, _, _, cx| t.show_diff_tab(cx)))
                    .child(Icon::new(IconName::File).xsmall())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .italic()
                            .child(name),
                    )
                    // Hidden rather than left out, so the name doesn't move when it shows.
                    .child(div().flex_none().invisible().group_hover(TAB_GROUP, Styled::visible).child(close)),
            )
            .when(selected, |d| {
                d.child(div().absolute().left_0().right_0().bottom_0().h(px(2.)).bg(look::tab_underline()))
            });
        Some(tab.into_any_element())
    }
}
