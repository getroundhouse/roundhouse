//! How a row of a tree is drawn: indented to its depth, with a line down the side for each folder it's in.

use gpui_kit::component::{Icon, IconName, h_flex};
use look::{Hand as _, IconSizing as _, Typeset as _};

use crate::file_icons::{ICON, file_icon};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    App, ElementId, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Stateful, Styled as _, div, px,
};

const PADDING: look::Space = look::Space::Medium;
/// The space between the things in a row.
const GAP: look::Space = look::Space::Small;
/// How big a folder's chevron is in Changes, among the folder's names in code.
const CHEVRON: look::IconSize = look::IconSize::Small;
/// How much further in from its left a chevron's drawing starts than a folder's icon's does.
const CHEVRON_ROOM: Pixels = px(2.);

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
    fn indent(self) -> Pixels {
        match self {
            List::AllFiles => ICON.size() + GAP.size(),
            // Under the folder's icon, as All files' are under its name, with each chevron's drawing level with the
            // icon's.
            List::Changes => CHEVRON.size() + GAP.size() - CHEVRON_ROOM,
        }
    }

    /// Where the line down a folder's contents sits: under the middle of what starts its row.
    fn guide(self) -> Pixels {
        match self {
            List::AllFiles => ICON.size() / 2.,
            List::Changes => CHEVRON.size() / 2.,
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "folders are never nested deep enough to lose precision")]
    fn inset(self, depth: usize) -> Pixels {
        PADDING.size() + self.indent() * depth as f32
    }
}

/// A row, indented to its depth, raised while what it stands for is showing, for its caller to fill and to click.
pub(crate) fn row_frame(id: String, list: List, depth: usize, showing: bool, cx: &App) -> Stateful<gpui_kit::Div> {
    let colors = look::colors(cx);
    let (border, hover, raised) = (colors.edge, colors.row_hover, colors.raised);
    let guides = (0..depth).map(move |level| {
        div().absolute().top_0().bottom_0().left(list.inset(level) + list.guide()).w(look::EDGE).bg(border)
    });
    h_flex()
        .id(ElementId::Name(id.into()))
        .relative()
        .flex_none()
        .w_full()
        .h(look::RowSize::Small.height())
        .pl(list.inset(depth))
        .pr(look::Space::Medium)
        .gap(GAP)
        .typeset(look::Type::Body)
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
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    row_frame(id, list, depth, false, cx)
        .when(list == List::Changes, |d| d.child(Icon::new(chevron).sized(CHEVRON).text_color(colors.text_faint)))
        .child(icon)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .typeset(look::Type::Code)
                .text_color(muted)
                .child(names.join(" / ")),
        )
}
