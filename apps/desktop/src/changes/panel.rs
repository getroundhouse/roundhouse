use super::{comments_mark, rows};
use crate::file_tree::{List, Row, chain, folder_row, row_frame, split};
use crate::patch::FileStatus;
use crate::review::{ChangedEntry, ChangedRow, Review};
use crate::shared_choice::{choose, chosen};
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled, Subscription, TestSupportExt as _, Window, div,
    uniform_list,
};
use look::{IconSizing as _, LineText, Placed, Typeset as _};
use rows::Layout;
use std::collections::HashSet;
use std::ops::Range;

#[derive(Clone, Debug)]
pub enum ChangesEvent {
    /// Show this file in the file tab.
    Open(ChangedEntry),
}

/// The layout chosen, which every workspace shares until the app quits.
fn layout(cx: &App) -> Layout {
    chosen(cx)
}

fn toggle_layout(cx: &mut App) {
    let next = match layout(cx) {
        Layout::List => Layout::Tree,
        Layout::Tree => Layout::List,
    };
    choose(next, cx);
}

pub struct Changes {
    pub(super) review: Entity<Review>,
    /// The file the file tab shows, as the pane says, which its row marks.
    pub(super) showing: Option<ChangedEntry>,
    /// Folders closed in the tree, by path.
    pub(super) closed: HashSet<String>,
    /// The rows as last laid out, which the list draws from as it scrolls, and the layout they're in. They're laid out
    /// again only when the files, the layout or the folders closed change, rather than at every frame, since a branch
    /// can change thousands of files.
    pub(super) rows: Vec<Row>,
    pub(super) laid_out: Option<Layout>,
    pub(super) _observing: Subscription,
}

impl EventEmitter<ChangesEvent> for Changes {}

impl Changes {
    pub fn new(review: Entity<Review>, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&review, |t, _, cx| {
            t.laid_out = None;
            cx.notify();
        });
        Self { review, showing: None, closed: HashSet::new(), rows: Vec::new(), laid_out: None, _observing: observing }
    }

    /// Marks the row of the file the file tab shows, or none.
    pub fn set_showing(&mut self, entry: Option<ChangedEntry>, cx: &mut Context<Self>) {
        self.showing = entry;
        cx.notify();
    }

    /// Closes or opens a folder's row, and each folder it stands for, so it stays closed if a new file splits it.
    pub(super) fn toggle_folder(&mut self, path: &str, names: &[String], cx: &mut Context<Self>) {
        let folders = chain(path, names);
        if self.closed.contains(path) {
            for folder in &folders {
                self.closed.remove(folder);
            }
        } else {
            self.closed.extend(folders);
        }
        self.laid_out = None;
        cx.notify();
    }

    /// The rows in `range`, for the list to draw.
    pub(super) fn draw_rows(
        &mut self,
        range: Range<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (open, layout) = (self.showing.clone(), layout(cx));
        let line_text = LineText::new(window, cx);
        let rows: Vec<Row> = self.rows.get(range).map_or_default(<[Row]>::to_vec);
        // Only the files in view, rather than every file the branch changes.
        let (files, commented) = {
            let review = self.review.read(cx);
            let files = review.listing().map_or_default(|listing| &*listing.rows);
            let file = |row: &Row| match row {
                Row::File { index, .. } => {
                    // Listed in order of path, the tracked file before the untracked one.
                    let at = |other: Option<usize>, file: &ChangedRow| {
                        other.and_then(|other| files.get(other)).is_some_and(|other| other.path == file.path)
                    };
                    files.get(*index).map(|file| {
                        let (before, after) = (at(index.checked_sub(1), file), at(index.checked_add(1), file));
                        (file.clone(), Twins { second: before, shared: before || after })
                    })
                }
                Row::Folder { .. } => None,
            };
            let in_view: Vec<Option<(ChangedRow, Twins)>> = rows.iter().map(file).collect();
            (in_view, comments_mark::commented(files, &review.comments))
        };
        rows.into_iter()
            .zip(files)
            .filter_map(|(row, file)| match row {
                Row::Folder { path, names, depth, open } => Some(
                    folder_row(
                        format!("folder-{path}"),
                        List::Changes,
                        &names,
                        depth,
                        open,
                        plain_folder(open, cx),
                        cx,
                    )
                    // An open folder's files show their own.
                    .children(comments_mark::mark(
                        &path,
                        if open { 0 } else { comments_mark::in_folder(&commented, &path) },
                        &line_text,
                        cx,
                    ))
                    .on_click(cx.listener(move |t, _, _, cx| t.toggle_folder(&path, &names, cx)))
                    .into_any_element(),
                ),
                Row::File { depth, .. } => {
                    let (file, twins) = file?;
                    let showing = open.as_ref().is_some_and(|open| open.is(&file.path, file.untracked, twins.shared));
                    let comments = commented.get(file.path.as_ref()).copied().unwrap_or_default();
                    let id = row_id(&file.path, twins.second);
                    Some(file_row(&file, id, (depth, layout), showing, comments, &line_text, cx))
                }
            })
            .collect()
    }
}

impl Render for Changes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = layout(cx);
        let review = self.review.read(cx);
        let truncated = review.listing().is_some_and(|listing| listing.truncated);
        let listing = review.listing().cloned();
        let changed = listing.as_ref().map(|listing| {
            if self.laid_out != Some(layout) {
                self.rows = rows(layout, listing.rows.iter().map(|file| file.path.as_ref()), &self.closed);
                self.laid_out = Some(layout);
            }
            listing.changed()
        });
        let muted = look::colors(cx).text_muted;
        let body = match changed {
            None => look::nothing_to_show("Loading…").into_any_element(),
            Some(0) => look::nothing_to_show("No changes yet").into_any_element(),
            Some(_) => uniform_list(
                "changed-files",
                self.rows.len(),
                cx.processor(|t, range, window, cx| t.draw_rows(range, window, cx)),
            )
            .flex_1()
            .w_full()
            .p(look::ROWS_INSET)
            .into_any_element(),
        };
        v_flex()
            .size_full()
            .when(truncated, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px(look::Space::Medium)
                        .py(look::Space::XSmall)
                        .typeset(look::Type::Detail)
                        .text_color(muted)
                        .child("Some untracked files aren't shown"),
                )
            })
            .child(body)
            .children(listing.filter(|listing| listing.left_out > 0).map(|listing| left_out(listing.left_out, cx)))
    }
}

/// The row under the list that says how many files the hub's summary leaves out, which aren't read, so a click on it
/// does nothing. Its words are a status to assistive apps, since the kit tells them nothing of plain text.
fn left_out(count: u64, cx: &App) -> AnyElement {
    let files = if count == 1 { "file" } else { "files" };
    let says = SharedString::from(format!("{} more {files} not shown", written_counts::whole(count)));
    let words = div()
        .id("changed-files-left-out-words")
        .role(Role::Status)
        .aria_label(says.clone())
        .typeset(look::Type::Body)
        .text_color(look::colors(cx).text_muted)
        .child(says)
        .test_support();
    let row = look::list_row("changed-files-left-out", look::RowSize::Small).child(words);
    div().flex_none().px(look::ROWS_INSET).pb(look::ROWS_INSET).child(row).into_any_element()
}

/// How many lines the files add and remove, and a way to change the layout, for the right end of the tabs above the
/// sidebar while Changes shows.
pub(crate) fn controls(changes: &Entity<Changes>, code: &Placed, cx: &App) -> AnyElement {
    let (added, removed) = changes.read(cx).review.read(cx).totals();
    let tree = layout(cx) == Layout::Tree;
    let changes = changes.downgrade();
    h_flex()
        .flex_none()
        .gap(look::Space::Small)
        // Out past the bar's room by the room around the button's icon, so the icon lines up with the rows' marks.
        .mr(-look::ControlSize::Regular.inset())
        // On the baseline of the tabs' names beside it.
        .child(look::lines_changed(look::LinesKind::Plain, added, removed).placed(code))
        .child(
            look::icon_button(
                "changes-layout",
                look::ButtonKind::Borderless,
                look::ControlSize::Regular,
                Lucide::ListTree,
            )
            .on(tree)
            .tooltip(if tree { "Show as a list" } else { "Show in folders" })
            .on_click(move |_, _, cx| {
                toggle_layout(cx);
                let _ = changes.update(cx, |_, cx| cx.notify());
            }),
        )
        .into_any_element()
}

/// How a file changed. A modified file's is yellow, as review tools show it, and the rest are muted.
fn status_icon(status: FileStatus, cx: &App) -> Icon {
    let colors = look::colors(cx);
    let (icon, color) = match status {
        FileStatus::Added => (Lucide::SquarePlus, colors.text_muted),
        FileStatus::Removed => (Lucide::SquareMinus, colors.text_muted),
        FileStatus::Renamed | FileStatus::Copied => (Lucide::SquareArrowRight, colors.text_muted),
        FileStatus::Modified => (Lucide::SquareDot, colors.modified),
    };
    Icon::new(icon).sized(look::IconSize::Small).text_color(color)
}

/// Whether a changed file's row shares its path with another's, and is the second of them, as the untracked one is.
#[derive(Clone, Copy)]
struct Twins {
    shared: bool,
    second: bool,
}

/// The element ID of the row of the file at `path`: `file-` and its path, but for the untracked file at a path a
/// tracked file is at too, as one taken out of git's index and left in the worktree is, whose row is the tracked one's
/// `:untracked`, so each row is one element.
fn row_id(path: &str, twinned: bool) -> String {
    if twinned { format!("file-{path}:untracked") } else { format!("file-{path}") }
}

/// A changed file's row, as the element `id`: where it is, then its name, and at the right end how many comments it
/// has, if any, how many lines it adds and removes, on the name's baseline, and how it changed.
fn file_row(
    file: &ChangedRow,
    id: String,
    (depth, layout): (usize, Layout),
    showing: bool,
    comments: usize,
    line_text: &LineText,
    cx: &mut Context<Changes>,
) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    let path = file.path.clone();
    let (folders, name) = split(&path);
    let (folders, name) = (folders.to_string(), name.to_string());
    let label = line_text
        .body
        .set(h_flex())
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        // The folders give way before the name does.
        .when(layout == Layout::List && !folders.is_empty(), |d| {
            d.child(div().flex_shrink(1.).min_w_0().overflow_hidden().text_ellipsis().text_color(muted).child(folders))
        })
        .child(div().flex_shrink_0().child(name));
    let status = status_icon(file.status, cx);
    let comments = comments_mark::mark(&path, comments, line_text, cx);
    let entry = file.entry();
    row_frame(id, List::Changes, depth, showing)
        .child(label)
        .child(
            h_flex()
                .flex_none()
                .gap(look::Space::Small)
                .children(comments)
                .child(look::lines_changed(look::LinesKind::Plain, file.added, file.removed).placed(&line_text.code))
                .child(status),
        )
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(ChangesEvent::Open(entry.clone()))))
        .into_any_element()
}

/// The kit's folder, open or closed, as big as All files' folders, since it stands for its row too. Changes leaves a
/// file's or folder's kind to All files, so a row says only how it changed.
fn plain_folder(open: bool, cx: &App) -> Icon {
    let folder = if open { IconName::FolderOpen } else { IconName::FolderClosed };
    Icon::new(folder).sized(look::IconSize::Large).text_color(look::colors(cx).text_muted)
}
