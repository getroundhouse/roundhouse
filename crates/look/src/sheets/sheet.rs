use std::rc::Rc;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement, Pixels, RenderOnce,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};

use crate::colors::colors;
use crate::lines::Lined as _;
use crate::shape::{Corner, Shaped as _};
use crate::space::Space;
use crate::typography::{Type, Typeset as _};

/// The grabber along a sheet's top: a short bar that says the sheet can be put away, as a phone's own sheets have.
/// It's the drawing's own size.
const GRABBER: (Pixels, Pixels) = (px(36.), px(5.));
/// The ID of the part of the screen a sheet dims, which a tap on puts the sheet away.
const BEHIND: &str = "sheet-behind";
/// The ID of a sheet with none of its own.
const SHEET: &str = "sheet";

use super::dimmed::OnTap as OnClose;

/// A sheet with nothing in it yet. It fills what it's put in, which is the screen, dimming it and sitting along its
/// bottom, so whatever owns what's open draws it last, over the rest.
pub fn sheet() -> Sheet {
    Sheet { title: None, clear: px(0.), id: None, on_close: None, children: Vec::new() }
}

/// A handheld's sheet, which [`sheet`] makes. What's put in it is laid out in a column, from its top.
#[derive(IntoElement)]
#[must_use]
pub struct Sheet {
    title: Option<SharedString>,
    clear: Pixels,
    id: Option<ElementId>,
    on_close: Option<OnClose>,
    children: Vec<AnyElement>,
}

impl Sheet {
    /// What it's about, in its middle under the grabber, with a line under it.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Taller by `bottom`, with what it holds over that, clear of what the device draws over the screen's bottom,
    /// such as its home indicator, which the sheet's surface runs down under.
    pub fn clear(mut self, bottom: Pixels) -> Self {
        self.clear = bottom;
        self
    }

    /// What a tap on what it dims does, which is to put it away.
    pub fn on_close(mut self, on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_close = Some(Rc::new(on_close));
        self
    }

    /// Found by a test as `id`.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

impl ParentElement for Sheet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Sheet {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = colors(cx);
        let grabber = div()
            .flex()
            .justify_center()
            .py(Space::Small)
            .child(div().w(GRABBER.0).h(GRABBER.1).rounded_full().bg(colors.control_edge));
        let title = self.title.map(|title| {
            div()
                .w_full()
                .px(Space::Large)
                .pb(Space::Medium)
                .typeset(Type::Body)
                .emphasis()
                .text_center()
                .truncate()
                .line_under(cx)
                .child(title)
        });
        let surface = v_flex()
            .id(self.id.unwrap_or_else(|| SHEET.into()))
            .test_support()
            // A press on the sheet isn't one on what it dims.
            .occlude()
            .w_full()
            .max_h_full()
            .pb(self.clear)
            .bg(colors.raised)
            .corners_top(Corner::Card)
            .line_over(cx)
            .text_color(colors.text)
            .typeset(Type::Body)
            .child(grabber)
            .children(title)
            .child(v_flex().id("sheet-holds").min_h_0().overflow_y_scroll().children(self.children));
        super::dimmed::over_what_it_dims(BEHIND, self.on_close, surface, cx)
    }
}
