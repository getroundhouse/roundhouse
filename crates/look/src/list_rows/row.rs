use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, px,
};

use crate::colors::colors;
use crate::hand::Hand as _;
use crate::heights::RowSize;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::theme::{CHOSEN, UNDER_POINTER};
use crate::typography::{Type, Typeset as _};
use gpui_kit::component::h_flex;

/// How far in from its list's edge a row sits, on every side of the list, so its rounded corners don't meet the edge.
pub const ROWS_INSET: Space = Space::XSmall;

/// A row of `size`, found by `id`, with nothing in it yet.
pub fn list_row(id: impl Into<ElementId>, size: RowSize) -> ListRow {
    ListRow {
        id: id.into(),
        size,
        chosen: false,
        wraps: false,
        indent: px(0.),
        role: None,
        tooltip: None,
        on_click: None,
        children: Vec::new(),
    }
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// One of what a list holds, which [`list_row`] makes. What's put in it is laid out in a line, from its start.
#[derive(IntoElement)]
#[must_use]
pub struct ListRow {
    id: ElementId,
    size: RowSize,
    chosen: bool,
    wraps: bool,
    indent: Pixels,
    role: Option<(Role, SharedString)>,
    tooltip: Option<SharedString>,
    on_click: Option<OnClick>,
    children: Vec<AnyElement>,
}

impl ListRow {
    /// Washed as what's chosen, such as the file showing or the page of Settings that's open.
    pub fn chosen(mut self, chosen: bool) -> Self {
        self.chosen = chosen;
        self
    }

    /// As tall as what it holds, at least its size's height, with its room above and below what it holds as well as
    /// at its ends, and what it holds at its top, such as a prompt of two lines in the outline.
    pub fn wraps(mut self) -> Self {
        self.wraps = true;
        self
    }

    /// Set in by `indent` after its start, such as a file in a folder.
    pub fn indent(mut self, indent: Pixels) -> Self {
        self.indent = indent;
        self
    }

    /// One option of a list to choose from, called `label` to assistive apps, which say whether it's chosen, such as a
    /// page of Settings.
    pub fn listed(mut self, label: impl Into<SharedString>) -> Self {
        self.role = Some((Role::ListBoxOption, label.into()));
        self
    }

    /// A link, called `label` to assistive apps, such as a page a web search found.
    pub fn link(mut self, label: impl Into<SharedString>) -> Self {
        self.role = Some((Role::Link, label.into()));
        self
    }

    /// What it names, while the pointer rests on it, where its words are cut short.
    pub fn tooltip(mut self, words: impl Into<SharedString>) -> Self {
        self.tooltip = Some(words.into());
        self
    }

    /// What a click on it does.
    pub fn on_click(mut self, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(on_click));
        self
    }
}

impl ParentElement for ListRow {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

/// The room at a row's ends and between what's in it, by how tall it is: a little more in a large row, which is a
/// table's, so its parts read apart across a wide pane.
fn room(size: RowSize) -> Space {
    match size {
        RowSize::Small | RowSize::Regular => Space::Small,
        RowSize::Large => Space::Medium,
    }
}

impl RenderOnce for ListRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let room = room(self.size);
        h_flex()
            .id(self.id)
            .test_support()
            .relative()
            .flex_none()
            .w_full()
            .map(|row| {
                if self.wraps {
                    row.min_h(self.size.height()).py(room).items_start()
                } else {
                    row.h(self.size.height())
                }
            })
            .pl(room.size() + self.indent)
            .pr(room)
            .gap(room)
            .corners(Corner::Control)
            .typeset(Type::Body)
            .hand()
            .when(self.chosen, |row| row.bg(colors.text.opacity(CHOSEN)))
            .when(!self.chosen, |row| row.hover(|row| row.bg(colors.text.opacity(UNDER_POINTER))))
            .when_some(self.role, |row, (role, label)| {
                let row = row.role(role).aria_label(label);
                if role == Role::ListBoxOption { row.aria_selected(self.chosen) } else { row }
            })
            .when_some(self.tooltip, |row, words| row.tooltip(crate::tooltip(words)))
            .when_some(self.on_click, |row, on_click| {
                row.on_click(move |event, window, cx| on_click(event, window, cx))
            })
            .children(self.children)
    }
}
