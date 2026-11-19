use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce, SharedString,
    Styled as _, TestSupportExt as _, Window, div, px,
};

use super::BarKind;
use crate::colors::colors;
use crate::heights::BarSize;
use crate::lines::Lined as _;
use crate::region::Region;
use crate::shape::EDGE;
use crate::typography::{Type, Typeset as _};
use crate::work::Work;

/// A bar of `kind` and `size`, with nothing in it yet.
pub fn bar(kind: BarKind, size: BarSize) -> Bar {
    Bar {
        kind,
        size,
        clear: (px(0.), px(0.)),
        work: None,
        id: None,
        group: None,
        children: Vec::new(),
        end: Vec::new(),
    }
}

/// A bar along the top of a pane, a sidebar or a card, which [`bar`] makes. What's put in it is laid out in a line
/// from its start, and what's put at its end with [`Bar::end`] from its end, with its edge under it all.
#[derive(IntoElement)]
#[must_use]
pub struct Bar {
    kind: BarKind,
    size: BarSize,
    clear: (Pixels, Pixels),
    work: Option<Work>,
    id: Option<ElementId>,
    group: Option<SharedString>,
    children: Vec<AnyElement>,
    end: Vec<AnyElement>,
}

impl Bar {
    /// Further in by `start` and `end`, clear of what's drawn over its ends, such as the window's buttons.
    pub fn clear(mut self, start: Pixels, end: Pixels) -> Self {
        self.clear = (start, end);
        self
    }

    /// Tinted in what `work`'s pull request's colour says, as the pull request's bar is.
    pub fn of_work(mut self, work: Work) -> Self {
        self.work = Some(work);
        self
    }

    /// Found by a test as `region`, the part of the window it is.
    pub fn region(mut self, region: Region) -> Self {
        self.id = Some(region.id().into());
        self
    }

    /// Found by a test as `id`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// The group what shows only while the pointer's on the bar is shown by, such as a button that copies its path,
    /// with `look::Button::shown_while_over`.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// `element` at its end, after the rest, such as what's done about what the bar says.
    pub fn end(mut self, element: impl IntoElement) -> Self {
        self.end.push(element.into_any_element());
        self
    }
}

impl ParentElement for Bar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Bar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let kind = self.kind;
        let (start, end) = self.clear;
        let row = h_flex()
            .size_full()
            .pl(kind.start().size() + start)
            .pr(BarKind::END.size() + end)
            .gap(kind.gap())
            .children(self.children)
            .when(!self.end.is_empty(), |row| row.child(div().flex_1()).children(self.end));
        let drawn = div()
            .relative()
            .flex_none()
            .w_full()
            .h(self.size.height())
            .typeset(Type::Body)
            .when_some(self.work, |bar, work| bar.bg(work.tint(colors)))
            .when_some(self.group, gpui_kit::InteractiveElement::group)
            // A row of tabs has its edge under the tabs rather than as a border, so the line under the one showing can
            // cover it. GPUI draws a border over what's inside it.
            .map(|bar| match kind {
                BarKind::Words => bar.line_under(cx).child(row),
                BarKind::Tabs => {
                    bar.child(div().absolute().left_0().right_0().bottom_0().h(EDGE).bg(colors.edge)).child(row)
                }
            });
        match self.id {
            Some(id) => drawn.id(id).test_support().into_any_element(),
            None => drawn.into_any_element(),
        }
    }
}
