//! How a row of a tree is drawn: indented to its depth, with a line down the side for each folder it's in.

use gpui_kit::component::{ActiveTheme as _, Icon, IconName, h_flex};
use look::Hand as _;

use crate::file_icons::{ICON_SIZE, file_icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Stateful, Styled as _, div, px,
};

pub(crate) const ROW_HEIGHT: Pixels = px(28.);
const PADDING: f32 = 12.;
/// The space between the things in a row.
const GAP: f32 = 6.;
/// How big a folder's chevron is in Changes.
const CHEVRON: f32 = 12.;

/// Which list a row is in, which says what starts a folder's row, how far what it holds sits inside it, and where the
/// line down that is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum List {
    /// All files, which draws a folder's icon alone, as an editor's tree of files does, the icon opening and closing
    /// with it, and starts what it holds under its name.
    AllFiles,
    /// Changes, which puts a chevron before a folder's icon.
    Changes,
}

impl List {
    /// How far a folder's contents sit inside it.
    fn indent(self) -> f32 {
        match self {
            List::AllFiles => ICON_SIZE + GAP,
            List::Changes => 16.,
        }
    }

    /// Where the line down a folder's contents sits: under the middle of what starts its row.
    fn guide(self) -> f32 {
        match self {
            List::AllFiles => ICON_SIZE / 2.,
            List::Changes => CHEVRON / 2.,
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "folders are never nested deep enough to lose precision")]
    fn inset(self, depth: usize) -> f32 {
        PADDING + depth as f32 * self.indent()
    }
}

/// A row, indented to its depth, raised while what it stands for is showing, for its caller to fill and to click.
pub(crate) fn row_frame(id: String, list: List, depth: usize, showing: bool, cx: &App) -> Stateful<gpui_kit::Div> {
    let colors = look::colors(cx);
    let (border, hover, raised) = (colors.edge, colors.row_hover, colors.raised);
    let guides = (0..depth).map(move |level| {
        div().absolute().top_0().bottom_0().left(px(list.inset(level) + list.guide())).w(px(1.)).bg(border)
    });
    h_flex()
        .id(ElementId::Name(id.into()))
        .relative()
        .flex_none()
        .w_full()
        .h(ROW_HEIGHT)
        .pl(px(list.inset(depth)))
        .pr_3()
        .gap(px(GAP))
        .text_sm()
        .hand()
        .when(showing, |d| d.bg(raised))
        .when(!showing, |d| d.hover(|s| s.bg(hover)))
        .children(guides)
}

/// A file's row in All files, which is the only list that shows a file's kind, with the icon for it, for its caller to
/// add its name to and to click.
pub(crate) fn file_frame(id: String, path: &str, depth: usize, showing: bool, cx: &App) -> Stateful<gpui_kit::Div> {
    row_frame(id, List::AllFiles, depth, showing, cx).child(file_icon(path, cx))
}

/// A folder's row, with the icon its caller gives it, after a chevron in Changes, and the names of the folders it
/// stands for, for its caller to click.
pub(crate) fn folder_row(
    id: String,
    list: List,
    names: &[String],
    depth: usize,
    open: bool,
    icon: impl IntoElement,
    cx: &App,
) -> Stateful<gpui_kit::Div> {
    let theme = cx.theme();
    let colors = look::colors(cx);
    let (muted, mono) = (colors.text_muted, theme.mono_font_family.clone());
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    row_frame(id, list, depth, false, cx)
        .when(list == List::Changes, |d| d.child(Icon::new(chevron).size(px(CHEVRON)).text_color(colors.text_faint)))
        .child(icon)
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
}
