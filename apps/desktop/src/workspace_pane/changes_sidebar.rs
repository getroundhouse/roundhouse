//! The sidebar of changes on the right: the pull request's bar, then the workspace's changes or all its files, as the
//! tabs under the bar choose. Clicking a file in either shows it in the workspace's file tab.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{AnyElement, App, AppContext as _, Context, IntoElement as _, ParentElement as _, Styled as _, div};

use super::{Diff, Opened, PaneEvent, WorkspacePane};
use crate::all_files::{AllFiles, AllFilesEvent};
use crate::changes::{Changes, ChangesEvent};
use crate::file_diff::{FileDiff, FileDiffEvent};
use crate::file_view::FileView;
use crate::review::{Review, ReviewEvent};
use crate::sidebar_tabs::{self, SidebarTab, TabShown};
use crate::sides::{self, Side, Sidebar};

impl WorkspacePane {
    /// The sidebar of changes shows, in every workspace.
    pub fn changes_open(&self, cx: &App) -> bool {
        Side::Right.chosen(cx).sidebar == Sidebar::Open
    }

    /// Shows or hides the sidebar of changes, in every workspace, reading again what it shows as it opens.
    pub fn toggle_changes(&mut self, cx: &mut Context<Self>) {
        Side::Right.toggle(cx);
        self.load_changes(cx);
        cx.notify();
    }

    /// Shows the workspace's changes or all its files in the sidebar of changes, reading again what it shows, as
    /// opening the sidebar does.
    pub fn show_sidebar_tab(&mut self, tab: SidebarTab, cx: &mut Context<Self>) {
        let Some(diff) = &mut self.diff else { return };
        diff.sidebar_tab.click(tab);
        self.load_changes(cx);
        cx.notify();
    }

    /// Which of the two the sidebar of changes shows, once the workspace's changes are read or a tab is clicked.
    pub fn sidebar_tab(&self) -> Option<SidebarTab> {
        self.diff.as_ref().and_then(|diff| diff.sidebar_tab.tab())
    }

    /// Tells the tab shown something that may move it, and lists the files if All files is what shows now.
    fn tell_tab(&mut self, tell: impl FnOnce(&mut TabShown), cx: &mut Context<Self>) {
        let Some(diff) = &mut self.diff else { return };
        let before = diff.sidebar_tab.tab();
        tell(&mut diff.sidebar_tab);
        if diff.sidebar_tab.tab() != before {
            self.list_files_if_shown(cx);
            cx.notify();
        }
    }

    /// Loads the diff, or loads it again, if the sidebar of changes or the file tab shows it, with what else they show
    /// of the worktree.
    pub(super) fn load_changes(&mut self, cx: &mut Context<Self>) {
        let open = self.changes_open(cx);
        match &self.diff {
            Some(diff) if open || diff.tab.is_some() => self.reread_worktree(cx),
            // The changes it starts reading choose the tab, and All files lists its files if it's the one.
            None if open => self.make_diff(cx),
            Some(_) | None => {}
        }
    }

    /// Reads again what the sidebar of changes and the file tab show of the worktree, for when it may have changed,
    /// such as at the end of a turn: the diff, all the files if they show, and the file the file tab shows, if it's
    /// opened from them.
    pub(super) fn reread_worktree(&mut self, cx: &mut Context<Self>) {
        let Some(diff) = &self.diff else { return };
        diff.review.update(cx, Review::refresh);
        if diff.tab.as_ref().is_some_and(|tab| tab.opened == Opened::File) {
            diff.viewer.update(cx, FileView::refresh);
        }
        self.list_files_if_shown(cx);
    }

    /// Reads again what the agent may have changed when a turn ends. The files are listed again only if the pane is on
    /// screen, and otherwise once it is, since listing the files of every workspace that shows All files at every
    /// turn would be wasted.
    pub(super) fn turn_ended(&mut self, cx: &mut Context<Self>) {
        if self.on_screen() {
            self.reread_worktree(cx);
            return;
        }
        let Some(diff) = &mut self.diff else { return };
        diff.files_behind = true;
        diff.review.update(cx, Review::refresh);
        if diff.tab.as_ref().is_some_and(|tab| tab.opened == Opened::File) {
            diff.viewer.update(cx, FileView::refresh);
        }
    }

    /// Lists the files again if a turn ended while the pane wasn't on screen, for when it comes on screen.
    pub(super) fn catch_up_files(&mut self, cx: &mut Context<Self>) {
        if self.diff.as_ref().is_some_and(|diff| diff.files_behind) {
            self.list_files_if_shown(cx);
        }
    }

    /// Lists the worktree's files again while All files shows, and not otherwise, since a big repository has
    /// thousands.
    fn list_files_if_shown(&mut self, cx: &mut Context<Self>) {
        let shown = self.changes_open(cx) && self.sidebar_tab() == Some(SidebarTab::AllFiles);
        let Some(diff) = self.diff.as_mut().filter(|_| shown) else { return };
        diff.files_behind = false;
        diff.all_files.update(cx, AllFiles::refresh);
    }

    /// Makes the diff, and the views of the worktree, of which the diff's start loading it.
    fn make_diff(&mut self, cx: &mut Context<Self>) {
        let (client, records, id) = (self.client.clone(), self.records.clone(), self.workspace.id.clone());
        let comments = self.comments.clone();
        let review = cx.new(|cx| Review::new(client.clone(), records, id.clone(), comments, cx));
        let file = cx.new(|cx| FileDiff::new(review.clone(), cx));
        let viewer = cx.new(|cx| FileView::new(client.clone(), id.clone(), cx));
        let changes = cx.new(|cx| Changes::new(review.clone(), cx));
        let all_files = cx.new(|_| AllFiles::new(client, id));
        let review_events = cx.subscribe(&review, |this, _, event: &ReviewEvent, cx| match event {
            ReviewEvent::Error(message) => {
                cx.emit(PaneEvent::Error(message.clone()));
                this.tell_tab(TabShown::read_failed, cx);
            }
            ReviewEvent::SendToAgent(prompt) => {
                if let Some(transcript) = this.transcript() {
                    transcript.update(cx, |t, cx| t.send_prompt(prompt.clone(), cx));
                }
                // So the agent can be seen taking them up.
                this.show_chat(cx);
            }
            ReviewEvent::Loaded => {
                let files = this.diff.as_ref().and_then(|diff| diff.review.read(cx).files.as_ref().map(Vec::len));
                if let Some(files) = files {
                    this.tell_tab(|tab| tab.read(files), cx);
                }
            }
        });
        let file_events = cx.subscribe(&file, |this, _, event: &FileDiffEvent, cx| match event {
            FileDiffEvent::AddToChat(text) => this.add_to_composer(text.clone(), cx),
        });
        let changes_events = cx.subscribe(&changes, |this, _, event: &ChangesEvent, cx| match event {
            ChangesEvent::Open(path) => this.open_diff(path.clone(), cx),
        });
        let all_files_events = cx.subscribe(&all_files, |this, _, event: &AllFilesEvent, cx| match event {
            AllFilesEvent::Open(path) => this.open_file(path.clone(), cx),
            AllFilesEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
            AllFilesEvent::Unsupported => this.tell_tab(TabShown::hub_cant_list_files, cx),
        });
        self.subscriptions.extend([review_events, file_events, changes_events, all_files_events]);
        let (tab, files_behind, sidebar_tab) = (None, false, TabShown::default());
        self.diff = Some(Diff { review, file, viewer, changes, all_files, sidebar_tab, tab, files_behind });
    }

    /// Whether there's a sidebar of changes to draw beside the chat, shown or hidden.
    pub(crate) fn lists_changes(&self) -> bool {
        self.diff.is_some()
    }

    /// The sidebar of changes, beside the chat and the file tab, with the pull request's bar at its top, level with the
    /// workspace's header, and the tabs that choose what it lists under that, level with the chat tabs. `None` while
    /// none of it shows, or there's nothing to show in it yet.
    pub(super) fn changes_sidebar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let diff = self.diff.as_ref()?;
        let shown = Side::Right.shown(cx)?;
        let act = cx.listener(|t, action, _, cx| t.act(*action, cx));
        let bar = crate::pull_request::bar(&self.pull_request, &self.busy, &self.workspace.id, act, cx);
        let changed = diff.review.read(cx).files.as_ref().map(Vec::len);
        let chosen = diff.sidebar_tab.tab();
        let (controls, list) = match chosen {
            Some(SidebarTab::Changes) => {
                (Some(crate::changes::controls(&diff.changes, cx)), Some(diff.changes.clone().into_any_element()))
            }
            Some(SidebarTab::AllFiles) => (None, Some(diff.all_files.clone().into_any_element())),
            // Neither, for the moment until the changes are read, rather than one that the other then replaces.
            None => (None, None),
        };
        let on_pick = cx.listener(|t, tab: &SidebarTab, _, cx| t.show_sidebar_tab(*tab, cx));
        let tabs = sidebar_tabs::bar(chosen, changed, controls, on_pick, cx);
        let theme = cx.theme();
        let column = v_flex().size_full().border_l_1().border_color(theme.border).bg(theme.sidebar);
        let column = column.child(bar).child(tabs).child(div().flex_1().min_h_0().children(list));
        Some(sides::frame(Side::Right, shown, column, cx).into_any_element())
    }
}
