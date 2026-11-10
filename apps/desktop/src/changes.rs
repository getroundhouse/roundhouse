//! The sidebar on the right: every file the workspace changes against its base, as a list or in folders, with the
//! comments waiting to be sent. Clicking a file shows its diff in the workspace's diff tab.

use std::collections::HashSet;
use std::ops::Range;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::diff::{DiffFile, DiffFileStatus};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, Entity, EventEmitter, FontWeight, Global, InteractiveElement, IntoElement,
    ParentElement, Pixels, Render, SharedString, Stateful, StatefulInteractiveElement, Styled, Subscription,
    TestSupportExt as _, Window, div, px, uniform_list,
};

use crate::review::Review;
use rows::{Layout, Row, rows, split};

mod rows;

/// Its left edge included. As wide as the sidebar of changes `docs/inspiration.md` measures, so the layout feels
/// familiar, and wide enough for a long path and its counts on one line.
pub(crate) const WIDTH: Pixels = px(432.);
/// As tall as the chat tabs, so the two line up across the window.
const BAR_HEIGHT: Pixels = px(40.);
const ROW_HEIGHT: Pixels = px(28.);
const PADDING: f32 = 12.;
/// How far a folder's contents sit inside it.
const INDENT: f32 = 16.;
/// Where the line down a folder's contents sits, under the middle of its chevron.
const GUIDE: f32 = 6.;

#[derive(Clone, Debug)]
pub enum ChangesEvent {
    /// Show this file in the diff tab.
    Open(SharedString),
}

/// The layout chosen, which every workspace shares until the app quits.
struct Chosen(Layout);

impl Global for Chosen {}

fn layout(cx: &App) -> Layout {
    cx.try_global::<Chosen>().map(|chosen| chosen.0).unwrap_or_default()
}

fn toggle_layout(cx: &mut App) {
    let next = match layout(cx) {
        Layout::List => Layout::Tree,
        Layout::Tree => Layout::List,
    };
    cx.set_global(Chosen(next));
}

pub struct Changes {
    review: Entity<Review>,
    /// The file the diff tab shows, as the pane says, which its row marks.
    showing: Option<SharedString>,
    /// Folders closed in the tree, by path.
    closed: HashSet<String>,
    /// The rows as last drawn, which the list draws from as it scrolls.
    rows: Vec<Row>,
    _observing: Subscription,
}

impl EventEmitter<ChangesEvent> for Changes {}

impl Changes {
    pub fn new(review: Entity<Review>, cx: &mut Context<Self>) -> Self {
        let observing = cx.observe(&review, |_, _, cx| cx.notify());
        Self { review, showing: None, closed: HashSet::new(), rows: Vec::new(), _observing: observing }
    }

    /// Marks the row of the file the diff tab shows, or none.
    pub fn set_showing(&mut self, path: Option<SharedString>, cx: &mut Context<Self>) {
        self.showing = path;
        cx.notify();
    }

    fn toggle_folder(&mut self, path: String, cx: &mut Context<Self>) {
        if !self.closed.remove(&path) {
            self.closed.insert(path);
        }
        cx.notify();
    }

    /// The rows in `range`, for the list to draw.
    fn draw_rows(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (files, open) = (self.review.read(cx).files.clone().unwrap_or_default(), self.showing.clone());
        let layout = layout(cx);
        let rows: Vec<Row> = self.rows.get(range).map(<[Row]>::to_vec).unwrap_or_default();
        rows.into_iter()
            .filter_map(|row| match row {
                Row::Folder { path, names, depth, open } => Some(folder_row(path, &names, depth, open, cx)),
                Row::File { index, depth } => {
                    let file = files.get(index)?;
                    let showing = open.as_ref() == Some(file.path());
                    Some(file_row(file, depth, layout, showing, cx))
                }
            })
            .collect()
    }

    /// How many comments are waiting, and the button that sends them.
    fn comments_bar(&self, count: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let review = self.review.downgrade();
        h_flex()
            .flex_none()
            .w_full()
            .h(BAR_HEIGHT)
            .px_3()
            .gap_2()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .child(format!("{count} comment{}", if count == 1 { "" } else { "s" })),
            )
            .child(Button::new("send-comments").primary().small().label("Send to agent").on_click(move |_, _, cx| {
                let _ = review.update(cx, Review::send_comments);
            }))
    }
}

impl Render for Changes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let review = self.review.read(cx);
        let (files, truncated, comments) = (review.files.clone(), review.truncated, review.comments.len());
        let muted = cx.theme().muted_foreground;
        let body = match &files {
            None => placeholder("Loading…", muted),
            Some(files) if files.is_empty() => placeholder("No changes yet", muted),
            Some(files) => {
                self.rows = rows(layout(cx), files.iter().map(|file| file.path().as_ref()), &self.closed);
                uniform_list("changed-files", self.rows.len(), cx.processor(|t, range, _, cx| t.draw_rows(range, cx)))
                    .flex_1()
                    .w_full()
                    .py_1()
                    .into_any_element()
            }
        };
        v_flex()
            .size_full()
            .child(summary(files.as_deref(), cx))
            .when(comments > 0, |d| d.child(self.comments_bar(comments, cx)))
            .when(truncated, |d| {
                d.child(
                    div()
                        .flex_none()
                        .px_3()
                        .py_1()
                        .text_xs()
                        .text_color(muted)
                        .child("Some untracked files aren't shown"),
                )
            })
            .child(body)
    }
}

/// How many files changed, and by how much, with a way to change the layout.
fn summary(files: Option<&[DiffFile]>, cx: &mut Context<Changes>) -> impl IntoElement {
    let tree = layout(cx) == Layout::Tree;
    let theme = cx.theme();
    let (border, muted, mono) = (theme.border, theme.muted_foreground, theme.mono_font_family.clone());
    let title = match files {
        Some(files) if !files.is_empty() => {
            format!("{} file{} changed", files.len(), if files.len() == 1 { "" } else { "s" })
        }
        _ => "Changes".to_string(),
    };
    let (added, removed) = files
        .unwrap_or_default()
        .iter()
        .fold((0, 0), |(added, removed), file| (added + file.additions(), removed + file.deletions()));
    h_flex()
        .flex_none()
        .w_full()
        .h(BAR_HEIGHT)
        .pl_3()
        .pr_2()
        .gap_2()
        .border_b_1()
        .border_color(border)
        .child(div().flex_1().min_w_0().text_sm().text_color(muted).whitespace_nowrap().child(title))
        .child(h_flex().flex_none().gap_1p5().text_xs().font_family(mono).children(counts(added, removed, cx)))
        .child(
            Button::new("changes-layout")
                .ghost()
                .xsmall()
                .icon(Lucide::ListTree)
                .selected(tree)
                .tooltip(if tree { "Show as a list" } else { "Show in folders" })
                .on_click(cx.listener(|_, _, _, cx| {
                    toggle_layout(cx);
                    cx.notify();
                })),
        )
}

/// Lines added and removed, leaving out whichever is none.
fn counts(added: usize, removed: usize, cx: &App) -> Vec<AnyElement> {
    let theme = cx.theme();
    let mut out = Vec::new();
    if added > 0 {
        out.push(div().text_color(theme.success).child(format!("+{added}")).into_any_element());
    }
    if removed > 0 {
        out.push(div().text_color(theme.danger).child(format!("-{removed}")).into_any_element());
    }
    out
}

/// How a file changed. A modified file's is yellow, as review tools show it, and the rest are muted.
fn status_icon(status: DiffFileStatus, cx: &App) -> Icon {
    let theme = cx.theme();
    let (icon, color) = match status {
        DiffFileStatus::Added => (Lucide::SquarePlus, theme.muted_foreground),
        DiffFileStatus::Deleted => (Lucide::SquareMinus, theme.muted_foreground),
        DiffFileStatus::Renamed | DiffFileStatus::Copied => (Lucide::SquareArrowRight, theme.muted_foreground),
        DiffFileStatus::Modified => (Lucide::SquareDot, theme.yellow),
        _ => (Lucide::SquareDot, theme.muted_foreground),
    };
    Icon::new(icon).xsmall().text_color(color)
}

#[expect(clippy::cast_precision_loss, reason = "folders are never nested deep enough to lose precision")]
fn indent(depth: usize) -> f32 {
    PADDING + depth as f32 * INDENT
}

/// A row, indented to its depth, with a line down the side for each folder it's in.
fn row_frame(id: String, depth: usize, showing: bool, cx: &App) -> Stateful<gpui_kit::Div> {
    let theme = cx.theme();
    let (border, hover, raised) = (theme.border, theme.list_hover, theme.secondary);
    let guides = (0..depth)
        .map(move |level| div().absolute().top_0().bottom_0().left(px(indent(level) + GUIDE)).w(px(1.)).bg(border));
    h_flex()
        .id(ElementId::Name(id.into()))
        .relative()
        .flex_none()
        .w_full()
        .h(ROW_HEIGHT)
        .pl(px(indent(depth)))
        .pr_3()
        .gap_1p5()
        .text_sm()
        .cursor_pointer()
        .when(showing, |d| d.bg(raised))
        .when(!showing, |d| d.hover(|s| s.bg(hover)))
        .children(guides)
}

fn file_row(file: &DiffFile, depth: usize, layout: Layout, showing: bool, cx: &mut Context<Changes>) -> AnyElement {
    let theme = cx.theme();
    let (muted, mono) = (theme.muted_foreground, theme.mono_font_family.clone());
    let path = file.path().clone();
    let (folders, name) = split(&path);
    let (folders, name) = (folders.to_string(), name.to_string());
    let label = h_flex()
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
    let counts = counts(file.additions(), file.deletions(), cx);
    row_frame(format!("file-{path}"), depth, showing, cx)
        .child(label)
        .child(h_flex().flex_none().gap_1p5().text_xs().font_family(mono).children(counts).child(status))
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(ChangesEvent::Open(path.clone()))))
        .test_support()
        .into_any_element()
}

fn folder_row(path: String, names: &[String], depth: usize, open: bool, cx: &mut Context<Changes>) -> AnyElement {
    let theme = cx.theme();
    let (muted, mono) = (theme.muted_foreground, theme.mono_font_family.clone());
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    let folder = if open { IconName::FolderOpen } else { IconName::FolderClosed };
    row_frame(format!("folder-{path}"), depth, false, cx)
        .child(Icon::new(chevron).xsmall().text_color(look::faint()))
        .child(Icon::new(folder).xsmall().text_color(muted))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_xs()
                .font_family(mono)
                .text_color(muted)
                .child(names.join(" / ")),
        )
        .on_click(cx.listener(move |t, _, _, cx| t.toggle_folder(path.clone(), cx)))
        .test_support()
        .into_any_element()
}

fn placeholder(text: &'static str, color: gpui_kit::Hsla) -> AnyElement {
    div().flex_1().w_full().flex().items_center().justify_center().text_color(color).child(text).into_any_element()
}
