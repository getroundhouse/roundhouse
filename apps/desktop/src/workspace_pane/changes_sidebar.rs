//! The sidebar of changes on the right: the pull request's bar, then the workspace's changes or all its files, as the
//! tabs under the bar choose. Clicking a file in either shows it in the workspace's file tab.

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Global, IntoElement as _, ParentElement as _, Styled as _, div,
};

use super::{Diff, Opened, PaneEvent, WorkspacePane};
use crate::all_files::{AllFiles, AllFilesEvent};
use crate::changes::{Changes, ChangesEvent};
use crate::file_diff::{FileDiff, FileDiffEvent};
use crate::file_view::FileView;
use crate::review::{Review, ReviewEvent};
use crate::sidebar_tabs::{self, SidebarTab};

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

    /// Shows or hides the sidebar of changes, in every workspace, reading again what it shows as it opens.
    pub fn toggle_changes(&mut self, cx: &mut Context<Self>) {
        let next = match sidebar(cx) {
            Sidebar::Open => Sidebar::Hidden,
            Sidebar::Hidden => Sidebar::Open,
        };
        cx.set_global(Chosen(next));
        self.load_changes(cx);
        cx.notify();
    }

    /// Shows the workspace's changes or all its files in the sidebar of changes, in every workspace, reading again what
    /// it shows, as opening the sidebar does.
    pub fn show_sidebar_tab(&mut self, tab: SidebarTab, cx: &mut Context<Self>) {
        sidebar_tabs::choose(tab, cx);
        self.load_changes(cx);
        cx.notify();
    }

    /// Loads the diff, or loads it again, if the sidebar of changes or the file tab shows it, with what else they show
    /// of the worktree.
    pub(super) fn load_changes(&mut self, cx: &mut Context<Self>) {
        let open = self.changes_open(cx);
        match &self.diff {
            Some(diff) if open || diff.tab.is_some() => self.reread_worktree(cx),
            None if open => {
                self.make_diff(cx);
                self.list_files_if_shown(cx);
            }
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
    /// screen, and otherwise once it is, since the tab chosen is every workspace's, and listing every workspace's
    /// files at every turn would be wasted.
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
        let shown = self.changes_open(cx) && sidebar_tabs::chosen(cx) == SidebarTab::AllFiles;
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
            ChangesEvent::Open(path) => this.open_diff(path.clone(), cx),
        });
        let all_files_events = cx.subscribe(&all_files, |this, _, event: &AllFilesEvent, cx| match event {
            AllFilesEvent::Open(path) => this.open_file(path.clone(), cx),
            AllFilesEvent::Error(message) => cx.emit(PaneEvent::Error(message.clone())),
        });
        self.subscriptions.extend([review_events, file_events, changes_events, all_files_events]);
        self.diff = Some(Diff { review, file, viewer, changes, all_files, tab: None, files_behind: false });
    }

    /// The sidebar of changes, beside the chat and the file tab, while it's open, with the pull request's bar at its
    /// top, level with the workspace's header, and the tabs that choose what it lists under that, level with the chat
    /// tabs.
    pub(super) fn changes_sidebar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let diff = self.diff.as_ref().filter(|_| self.changes_open(cx))?;
        let act = cx.listener(|t, action, _, cx| t.act(*action, cx));
        let bar = crate::pull_request::bar(&self.pull_request, &self.busy, &self.workspace.id, act, cx);
        let changed = diff.review.read(cx).files.as_ref().map(Vec::len);
        let (controls, list) = match sidebar_tabs::chosen(cx) {
            SidebarTab::Changes => {
                (Some(crate::changes::controls(&diff.changes, cx)), diff.changes.clone().into_any_element())
            }
            SidebarTab::AllFiles => (None, diff.all_files.clone().into_any_element()),
        };
        let on_pick = cx.listener(|t, tab: &SidebarTab, _, cx| t.show_sidebar_tab(*tab, cx));
        let tabs = sidebar_tabs::bar(changed, controls, on_pick, cx);
        let theme = cx.theme();
        let column = v_flex().flex_none().w(crate::changes::WIDTH).h_full().border_l_1().border_color(theme.border);
        Some(
            column
                .bg(theme.sidebar)
                .child(bar)
                .child(tabs)
                .child(div().flex_1().min_h_0().child(list))
                .into_any_element(),
        )
    }
}
