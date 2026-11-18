//! What a document does in each frame for its selection, around the list of its blocks, as the chat's list
//! does for the chat's: it ends a drag it wasn't drawn for, scrolls on under a pointer held past its top or bottom,
//! hears a press under the text, and once the text has said where it was drawn, moves a drag's far end to what's
//! under the pointer.

use gpui_kit::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId,
    IntoElement, LayoutId, ListState, Pixels, Window,
};

use crate::selection::Selection;

/// The list of a document's blocks, `scrolled`, by `list`, with what its selection needs done each frame.
pub(super) struct Frames {
    pub(super) scrolled: AnyElement,
    pub(super) list: ListState,
    pub(super) selection: Selection,
}

impl IntoElement for Frames {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Frames {
    type RequestLayoutState = ();
    /// Where the document hears the pointer.
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(ElementId::Name("document-frames".into()))
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
        (self.scrolled.request_layout(window, cx), ())
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
        if let Some(by) = self.selection.frame_started(id, bounds, window, cx).scroll_by {
            self.list.scroll_by(by);
        }
        // Under the text, so what's in it, such as a link, takes a press before the selection does.
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
