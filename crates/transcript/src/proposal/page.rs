//! A page drawn whole, as an element, whose texts are selected as a chat's are: what the chat's list does for its
//! rows, for what scrolls by GPUI's own handle. A press on its text starts a selection, a drag moves its far end, and
//! a pointer held past its top or bottom scrolls it on, as `crate::selection` says.

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, LayoutId, Pixels, ScrollHandle, Style, Window, point, px, relative,
};

use crate::selection::Selection;

/// What scrolls, as a child, with what's selected of the texts it draws.
pub(super) struct PageElement {
    pub(super) id: ElementId,
    /// What the page scrolls by, which a drag past its edge moves.
    pub(super) scroll: ScrollHandle,
    pub(super) selection: Selection,
    /// The area that scrolls, filling the page, with the texts in it.
    pub(super) scrolled: AnyElement,
}

impl IntoElement for PageElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PageElement {
    type RequestLayoutState = ();
    /// Where the page hears the pointer.
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let scrolled = self.scrolled.request_layout(window, cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.min_size.height = px(0.).into();
        (window.request_layout(style, [scrolled], cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        self.selection.new_frame();
        if let Some(by) = self.selection.frame_started(id, bounds, window, cx).scroll_by {
            let (offset, furthest) = (self.scroll.offset(), self.scroll.max_offset().y);
            self.scroll.set_offset(point(offset.x, (offset.y - by).clamp(-furthest, px(0.))));
        }
        // Before what's on the page, so what takes a press there, such as a button, has it first.
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.scrolled.prepaint(window, cx);
        self.selection.texts_laid_out(bounds, window);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.selection.hear_the_pointer(hitbox, bounds, window);
        self.scrolled.paint(window, cx);
    }
}
