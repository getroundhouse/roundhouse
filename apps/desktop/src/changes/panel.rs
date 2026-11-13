use super::rows;
use crate::file_tree::{List, Row, chain, folder_row, row_frame, split};
use crate::patch::{FilePatch, FileStatus};
use crate::review::Review;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, Global, IntoElement, ParentElement, Pixels, Render, SharedString,
    StatefulInteractiveElement, Styled, Subscription, TestSupportExt as _, Window, div, uniform_list,
};
use look::{Hand as _, LineText, Placed, Sizing as _, Typeset as _};
use rows::Layout;
use std::collections::HashSet;
use std::ops::Range;

/// As tall as the tabs above it.
const BAR_HEIGHT: Pixels = crate::sidebar_tabs::HEIGHT;

#[derive(Clone, Debug)]
pub enum ChangesEvent {
    /// Show this file in the file tab.
    Open(SharedString),
}

/// The layout chosen, which every workspace shares until the app quits.
struct Chosen(pub(super) Layout);

impl Global for Chosen {}

fn layout(cx: &App) -> Layout {
    cx.try_global::<Chosen>().map_or_default(|chosen| chosen.0)
}

fn toggle_layout(cx: &mut App) {
    let next = match layout(cx) {
        Layout::List => Layout::Tree,
        Layout::Tree => Layout::List,
    };
    cx.set_global(Chosen(next));
}

pub struct Changes {
    pub(super) review: Entity<Review>,
    /// The file the file tab shows, as the pane says, which its row marks.
    pub(super) showing: Option<SharedString>,
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
    pub fn set_showing(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        self.showing = path;
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
        let files: Vec<Option<FilePatch>> = {
            let files = self.review.read(cx).files.as_deref().unwrap_or_default();
            let file = |row: &Row| match row {
                Row::File { index, .. } => files.get(*index).cloned(),
                Row::Folder { .. } => None,
            };
            rows.iter().map(file).collect()
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
                    .on_click(cx.listener(move |t, _, _, cx| t.toggle_folder(&path, &names, cx)))
                    .test_support()
                    .into_any_element(),
                ),
                Row::File { depth, .. } => {
                    let file = file?;
                    let showing = open.as_ref() == Some(file.path());
                    Some(file_row(&file, depth, layout, showing, &line_text, cx))
                }
            })
            .collect()
    }

    /// How many comments are waiting, and the button that sends them.
    pub(super) fn comments_bar(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let review = self.review.downgrade();
        h_flex()
            .flex_none()
            .w_full()
            .h(BAR_HEIGHT)
            .px_3()
            .gap_2()
            .justify_between()
            .border_b_1()
            .border_color(look::colors(cx).edge)
            .child(
                div()
                    .typeset(look::Type::Body)
                    .emphasis()
                    .child(format!("{count} comment{}", if count == 1 { "" } else { "s" })),
            )
            .child(
                Button::new("send-comments")
                    .hand()
                    .primary()
                    .sized(look::ControlSize::Regular)
                    .label("Send to agent")
                    .on_click(move |_, _, cx| {
                        let _ = review.update(cx, Review::send_comments);
                    }),
            )
    }
}

impl Render for Changes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = layout(cx);
        let review = self.review.read(cx);
        let (truncated, comments) = (review.truncated, review.comments.len());
        let changed = review.files.as_ref().map(|files| {
            if self.laid_out != Some(layout) {
                self.rows = rows(layout, files.iter().map(|file| file.path().as_ref()), &self.closed);
                self.laid_out = Some(layout);
            }
            files.len()
        });
        let muted = look::colors(cx).text_muted;
        let body = match changed {
            None => placeholder("Loading…", muted),
            Some(0) => placeholder("No changes yet", muted),
            Some(_) => uniform_list(
                "changed-files",
                self.rows.len(),
                cx.processor(|t, range, window, cx| t.draw_rows(range, window, cx)),
            )
            .flex_1()
            .w_full()
            .py_1()
            .into_any_element(),
        };
        v_flex()
            .size_full()
            .when(comments > 0, |d| d.child(self.comments_bar(comments, cx)))
            .when(truncated, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px_3()
                        .py_1()
                        .typeset(look::Type::Detail)
                        .text_color(muted)
                        .child("Some untracked files aren't shown"),
                )
            })
            .child(body)
    }
}

/// How many lines the files add and remove, and a way to change the layout, for the right end of the tabs above the
/// sidebar while Changes shows.
pub(crate) fn controls(changes: &Entity<Changes>, code: &Placed, cx: &App) -> AnyElement {
    let files = changes.read(cx).review.read(cx).files.iter().flatten();
    let (added, removed) =
        files.fold((0, 0), |(added, removed), file| (added + file.added(), removed + file.removed()));
    let tree = layout(cx) == Layout::Tree;
    let changes = changes.downgrade();
    h_flex()
        .flex_none()
        .gap_2()
        // Out past the bar's room by the room around the button's icon, so the icon lines up with the rows' marks.
        .mr(-look::ControlSize::Regular.inset())
        // On the baseline of the tabs' names beside it.
        .child(code.set(h_flex().flex_none().gap_1p5().children(counts(added, removed, cx))))
        .child(
            Button::new("changes-layout")
                .hand()
                .ghost()
                .sized_square(look::ControlSize::Regular)
                .icon(Lucide::ListTree)
                .selected(tree)
                .tooltip(if tree { "Show as a list" } else { "Show in folders" })
                .on_click(move |_, _, cx| {
                    toggle_layout(cx);
                    let _ = changes.update(cx, |_, cx| cx.notify());
                }),
        )
        .into_any_element()
}

/// Lines added and removed, leaving out whichever is none.
fn counts(added: usize, removed: usize, cx: &App) -> Vec<AnyElement> {
    let colors = look::colors(cx);
    let mut out = Vec::new();
    if added > 0 {
        out.push(div().text_color(colors.added).child(format!("+{added}")).into_any_element());
    }
    if removed > 0 {
        out.push(div().text_color(colors.removed).child(format!("-{removed}")).into_any_element());
    }
    out
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
    Icon::new(icon).xsmall().text_color(color)
}

/// A changed file's row: where it is, then its name, and at the right end how many lines it adds and removes, on the
/// name's baseline, and how it changed.
fn file_row(
    file: &FilePatch,
    depth: usize,
    layout: Layout,
    showing: bool,
    line_text: &LineText,
    cx: &mut Context<Changes>,
) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    let path = file.path().clone();
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
    let status = status_icon(file.status(), cx);
    let counts = counts(file.added(), file.removed(), cx);
    row_frame(format!("file-{path}"), List::Changes, depth, showing, cx)
        .child(label)
        .child(
            h_flex().flex_none().gap_1p5().child(line_text.code.set(h_flex().gap_1p5().children(counts))).child(status),
        )
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(ChangesEvent::Open(path.clone()))))
        .test_support()
        .into_any_element()
}

/// The kit's small folder, open or closed. Changes leaves a file's or folder's kind to All files, so a row says only
/// how it changed.
fn plain_folder(open: bool, cx: &App) -> Icon {
    let folder = if open { IconName::FolderOpen } else { IconName::FolderClosed };
    Icon::new(folder).xsmall().text_color(look::colors(cx).text_muted)
}

fn placeholder(text: &'static str, color: gpui_kit::Hsla) -> AnyElement {
    div().flex_1().w_full().flex().items_center().justify_center().text_color(color).child(text).into_any_element()
}
