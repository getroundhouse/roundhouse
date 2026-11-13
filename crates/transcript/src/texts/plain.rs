//! The plain text a call shows, such as a command, what it printed and a question's answers, drawn with the
//! transcript's selection, as its Markdown is, rather than the kit's selectable text, which belongs to the window and
//! lets go of what's selected once a row holding either end of it isn't painted. A plain text is one leaf, what it
//! says as find searches it, which a row lays out as it likes: whole, or in `Pieces`, such as a line of what a command
//! printed at a time. What's selected is painted behind what draws it, as `Pieces` says.

use gpui_kit::base::Theme;
use gpui_kit::{
    AnyElement, App, Bounds, CursorStyle, Element, ElementId, GlobalElementId, Hitbox, HitboxBehavior,
    InspectorElementId, IntoElement, LayoutId, Pixels, SharedString, Window, fill,
};

use super::{Place, Text, TextAt, TextKey, Texts};
use crate::selection::{Geometry, Pieces, Selection};

/// One of the chat's plain texts, as a row draws it: where it goes, what it's known by, the area that scrolls it, what
/// draws it, what it says, and how what draws it lays that out.
pub(crate) struct Plain {
    pub(crate) at: Place,
    /// What it's known by, which only it is, and which stays the same while it's the same text.
    pub(crate) element: ElementId,
    pub(crate) area: ElementId,
    pub(crate) drawn: AnyElement,
    /// What it says, as find searches it, which a copy takes.
    pub(crate) said: SharedString,
    /// How `drawn` lays out what it says.
    pub(crate) pieces: Pieces,
}

impl Texts {
    /// Draws `plain`, and keeps what it says for a copy.
    pub(crate) fn draw_plain(&self, selection: &Selection, plain: Plain) -> AnyElement {
        let Plain { at, element, area, drawn, said, pieces } = plain;
        let known = TextKey(element);
        let mut store = self.0.borrow_mut();
        let at = store.noted(&known, at);
        store.texts.insert(known.clone(), Text::Plain { said: said.clone() });
        drop(store);
        PlainElement { known, at, area, drawn, said, pieces, selection: selection.clone() }.into_any_element()
    }
}

struct PlainElement {
    known: TextKey,
    at: TextAt,
    area: ElementId,
    drawn: AnyElement,
    said: SharedString,
    pieces: Pieces,
    selection: Selection,
}

impl IntoElement for PlainElement {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for PlainElement {
    type RequestLayoutState = ();
    /// Where the pointer is over it, which shows it's text to select.
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
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
        (self.drawn.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        (): &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        // Which places each piece where it's drawn, so it's where the selection is told it is.
        self.drawn.prepaint(window, cx);
        debug_assert!(self.pieces.lay_out(&self.said), "{:?} isn't laid out as it says {:?}", self.known, self.said);
        // What shows of it, which an area that scrolls it, or the list, can cut short.
        let shows = bounds.intersect(&window.content_mask().bounds);
        let geometry = [(0, Geometry::Plain(self.pieces.clone()))];
        self.selection.drawn(self.known.clone(), self.at, shows, Some(self.area.clone()), geometry);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        (): &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.set_cursor_style(CursorStyle::IBeam, hitbox);
        let color = Theme::global(cx).tokens.colors.selection;
        for (_, range) in self.selection.selected_in(self.at, [self.said.as_ref()]) {
            for bounds in self.pieces.selected_boxes(range, &self.said) {
                window.paint_quad(fill(bounds, color));
            }
        }
        self.drawn.paint(window, cx);
    }
}
