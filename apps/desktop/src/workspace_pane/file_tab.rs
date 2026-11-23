//! The workspace's one file tab, which shows the file last clicked in the sidebar of changes: its diff when it's
//! clicked in Changes, and the file itself when it's clicked in All files. Clicking another file shows that one in the
//! same tab, so reviewing a branch never leaves a trail of tabs.

use gpui_kit::component::{Icon, IconName};
use gpui_kit::{AnyElement, Context, IntoElement as _, SharedString};
use look::IconSizing as _;
use std::path::PathBuf;

use super::{Showing, WorkspacePane, tab};
use crate::file_diff::FileDiff;

/// What the file tab shows of its file, which depends on where it was opened from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    /// Its diff, opened from Changes.
    Diff,
    /// The file as it is in the worktree, opened from All files.
    File,
}

/// The file tab, while it's open.
pub(super) struct FileTab {
    pub(super) path: SharedString,
    pub(super) opened: Opened,
    /// It covers the active chat.
    pub(super) in_front: bool,
}

impl WorkspacePane {
    /// The file the file tab shows, if it's open.
    pub fn file_tab(&self) -> Option<SharedString> {
        self.diff.as_ref()?.tab.as_ref().map(|tab| tab.path.clone())
    }

    /// What the file tab shows of its file, if it's open.
    pub fn file_tab_opened(&self) -> Option<Opened> {
        self.diff.as_ref()?.tab.as_ref().map(|tab| tab.opened)
    }

    /// Shows a file's diff in the file tab, opening the tab if it isn't, and brings it to the front.
    pub fn open_diff(&mut self, path: SharedString, cx: &mut Context<Self>) {
        self.set_file_tab(Some((Opened::Diff, path)), cx);
        self.bring_to_front(Showing::File, cx);
    }

    /// Shows a file as it is in the worktree in the file tab, opening the tab if it isn't, and brings it to the front.
    pub fn open_file(&mut self, path: SharedString, cx: &mut Context<Self>) {
        self.set_file_tab(Some((Opened::File, path)), cx);
        self.bring_to_front(Showing::File, cx);
    }

    pub fn show_file_tab(&mut self, cx: &mut Context<Self>) {
        self.bring_to_front(Showing::File, cx);
    }

    /// Closes the file tab, folding what was opened of each file's diff, and shows the chat it covered.
    pub fn close_file_tab(&mut self, cx: &mut Context<Self>) {
        self.set_file_tab(None, cx);
        if let Some(diff) = &self.diff {
            diff.file.update(cx, FileDiff::fold_all);
        }
        self.bring_to_front(Showing::Chat, cx);
    }

    /// Opens the file tab on a file, in place of the one it showed, or closes it. It's the only way the tab's file
    /// changes, so the tab's view and the sidebar's marked row always say the same file, and only one of the two views
    /// and one of the two lists has it.
    fn set_file_tab(&mut self, tab: Option<(Opened, SharedString)>, cx: &mut Context<Self>) {
        let workspace = self.workspace();
        let worktree_here = workspace.provider.is_local().then(|| workspace.path.clone().map(PathBuf::from)).flatten();
        let Some(diff) = &mut self.diff else { return };
        let in_front = diff.tab.as_ref().is_some_and(|tab| tab.in_front);
        diff.tab = tab.clone().map(|(opened, path)| FileTab { path, opened, in_front });
        let (diffed, viewed) = match tab {
            Some((Opened::Diff, path)) => (Some(path), None),
            Some((Opened::File, path)) => (None, Some(path)),
            None => (None, None),
        };
        diff.file.update(cx, |f, cx| f.show(diffed.clone(), cx));
        diff.changes.update(cx, |c, cx| c.set_showing(diffed, cx));
        diff.viewer.update(cx, |v, cx| {
            v.set_worktree_here(worktree_here);
            v.show(viewed.clone(), cx);
        });
        diff.all_files.update(cx, |a, cx| a.set_showing(viewed, cx));
    }

    /// Adds a comment to what's typed in the active chat's composer, on a line of its own, and shows the chat with the
    /// composer ready to say more.
    pub(super) fn add_to_composer(&mut self, text: String, cx: &mut Context<Self>) {
        self.show_chat(cx);
        self.with_window(cx, move |t, window, cx| {
            let typed = t.composer.read(cx).text().to_string();
            let joined = if typed.trim().is_empty() { text } else { format!("{}\n{text}", typed.trim_end()) };
            let end = joined.len();
            t.composer.update(cx, |c, cx| {
                c.set_value(joined, window, cx);
                c.set_selected_range(end..end, cx);
            });
            t.keep_typed(true, cx);
        });
    }

    /// The file tab, before the chats, named for the file it shows, whether it shows its diff or the file itself. Its
    /// name is in italics, because it's replaced rather than added to. It's built as a chat's tab is, with the same
    /// width, underline and close button, so the row reads as one row of tabs.
    pub(super) fn file_tab_element(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (path, opened) = (self.file_tab()?, self.file_tab_opened()?);
        let name = path.rsplit('/').next().unwrap_or_default().to_string();
        let selected = self.showing() == Showing::File;
        let label = match opened {
            Opened::Diff => format!("Diff of {path}"),
            Opened::File => path.to_string(),
        };
        let close = tab::close_button("close-file-tab", "Close file tab", cx, |t, _, cx| t.close_file_tab(cx));
        let tab = look::tab("show-file-tab", name, look::TabWidth::Even)
            .label(label)
            .mark(Icon::new(IconName::File).sized(look::IconSize::Large))
            .replaced()
            .showing(selected)
            .closes(close)
            .on_click(cx.listener(|t, _, _, cx| t.show_file_tab(cx)));
        // Two files can share a name, so the whole path is on the bar under the tab, with the button that copies it.
        Some(tab.into_any_element())
    }
}
