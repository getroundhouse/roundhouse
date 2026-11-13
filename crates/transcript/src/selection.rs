//! What's selected of the chat's texts that `crate::texts` draws, which the transcript holds rather than the window,
//! so it outlasts the rows it's in being drawn. The kit's selection is the window's, over the texts painted in a frame,
//! and lets go of what's selected once a text holding either end of it isn't painted, which in a list that draws only
//! what's in sight is as soon as that text's row leaves the window.
//!
//! Each end is a place in a text: which text, which of its leaves, and a byte in that leaf. The text is named by what
//! its element is known by, which stays the same as it moves, as a reply does from being written to its record, so an
//! end stays in it, and `crate::texts` says where it is now, which orders the ends. Each frame, every text that's drawn
//! says what it's known by and where its leaves are, so a point in the window can be turned into a place in it. A
//! press on a text starts a selection, a drag moves its far end, and with the pointer past the top or bottom of the
//! list, the list scrolls on, faster the further past it is, for as long as the button is held. A copy reads the texts
//! between the ends as they were parsed, or what a plain text said when it was drawn, drawn now or not, which is each
//! the chat has loaded, and each it loaded as the selection was made, since the texts from one end to the other are
//! kept, as is the one a press was on, for a shift-click to extend from. `crate::texts` draws them.
//!
//! It's the mouse's: a press, a drag and a release. The kit's selection also takes a touch screen's long press and
//! handles, which a transcript on a phone will need here.

use std::cell::RefCell;
use std::ops::{Range, RangeInclusive};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui_kit::base::TextSelection;
use gpui_kit::base::input::{Copy, SelectAll};
use gpui_kit::{
    App, Bounds, ClickEvent, ClipboardItem, Div, ElementId, FocusHandle, InteractiveElement as _, MouseButton, Pixels,
    Point, SharedString, Window, point, px,
};
use look::motion::ScrollingOn;

use crate::texts::{Leaves, TextAt, TextKey, Texts};

mod geometry;
mod pieces;
mod places;
mod pointer;
#[cfg(test)]
mod test_layouts;
#[cfg(test)]
mod tests;

pub(crate) use geometry::Geometry;
pub(crate) use pieces::Pieces;
use places::{copied, line_around, selected, spot_at, text_under, word_around};

/// Where the keys that copy what's selected and select everything are bound, which is the transcript once a press in it
/// has focused it.
pub const KEY_CONTEXT: &str = "Transcript";

/// A place in a text, between two characters, in the chat's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Spot {
    at: TextAt,
    leaf: u64,
    byte: usize,
}

/// An end of what's selected: a place in the text known as `text`, wherever that is now.
#[derive(Clone, Debug, PartialEq, Eq)]
struct End {
    text: TextKey,
    leaf: u64,
    byte: usize,
}

impl End {
    /// The place `spot` names, in the text known as `text`, which is there now.
    fn at(text: TextKey, spot: Spot) -> Self {
        Self { text, leaf: spot.leaf, byte: spot.byte }
    }
}

/// What a press selects, and a drag from it goes by: a character, a word for a double click, or for a triple, a whole
/// paragraph, heading, cell or code block of Markdown, or a line of plain text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Unit {
    Character,
    Word,
    Paragraph,
}

/// The selection in one transcript. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Selection {
    inner: Rc<RefCell<Selecting>>,
    texts: Texts,
    /// The transcript's, which a press on its text takes, so a copy and a select all come to it.
    focus: FocusHandle,
}

#[derive(Default)]
struct Selecting {
    /// Where it started and where it's got to, in either order. Nothing's selected while they're the same.
    ends: Option<(End, End)>,
    /// Whether anything is, which a link asks as the button comes up over it, from a handler the kit has be `Send`.
    any: Arc<AtomicBool>,
    drag: Option<Drag>,
    /// The texts drawn in this frame, in the order they were drawn, which is the chat's.
    drawn: Vec<Drawn>,
}

struct Drag {
    pointer: Point<Pixels>,
    unit: Unit,
    /// What the press chose, which stays selected however the drag goes.
    chosen: (End, End),
    /// The list scrolling on under the pointer past its edge.
    scrolling: ScrollingOn,
    /// The area that scrolls the text the press was on, if it's in one.
    area: Option<ElementId>,
    /// That area scrolling on under the pointer past its edge.
    area_scrolling: ScrollingOn,
}

/// A text drawn in this frame, what it's known by and where it is, what shows of it, the area that scrolls it, if it's
/// in one, and where each of its leaves is, by key.
struct Drawn {
    text: TextKey,
    at: TextAt,
    bounds: Bounds<Pixels>,
    area: Option<ElementId>,
    leaves: Vec<(u64, Geometry)>,
}

impl Selecting {
    fn set(&mut self, ends: Option<(End, End)>) {
        self.any.store(ends.as_ref().is_some_and(|(a, b)| a != b), Ordering::Relaxed);
        self.ends = ends;
    }
}

impl Selection {
    pub(crate) fn new(texts: Texts, focus: FocusHandle) -> Self {
        Self { inner: Rc::default(), texts, focus }
    }

    /// `chat`, the transcript's, with its focus, and the keys that copy what's selected and select everything. GPUI
    /// gives focus to what tracks it on a press in it that nothing else takes, so a press anywhere in the chat gives
    /// it the focus, rather than the window, which tracks it everywhere else.
    pub(crate) fn with_keys(&self, chat: Div) -> Div {
        let (copying, selecting) = (self.clone(), self.clone());
        (chat.key_context(KEY_CONTEXT).track_focus(&self.focus))
            .on_action(move |_: &Copy, _, cx| copying.copy(cx))
            .on_action(move |_: &SelectAll, window, cx| selecting.select_all(window, cx))
    }

    /// The list has started drawing a frame: forgets the texts drawn in the last, before this one's are, and tells the
    /// texts, which keep the blocks of those drawn lately.
    pub(crate) fn new_frame(&self) {
        self.inner.borrow_mut().drawn.clear();
        self.texts.new_frame();
    }

    /// Notes that the text known as `text`, at `at`, shows in `bounds`, in `area` if one scrolls it, with its leaves
    /// where `leaves` says. A leaf that doesn't show, such as one an area that scrolls the text has moved out of sight,
    /// can't be pressed on or dragged to.
    pub(crate) fn drawn(
        &self,
        text: TextKey,
        at: TextAt,
        bounds: Bounds<Pixels>,
        area: Option<ElementId>,
        leaves: impl IntoIterator<Item = (u64, Geometry)>,
    ) {
        let showing = leaves.into_iter().filter(|(_, geometry)| geometry.shows(bounds));
        let mut leaves: Vec<(u64, Geometry)> = showing.collect();
        leaves.sort_by_key(|(key, _)| *key);
        let mut selecting = self.inner.borrow_mut();
        debug_assert!(
            selecting.drawn.last().is_none_or(|before| before.at.of != at.of || before.at < at),
            "{text:?} is painted after the text at {:?}, and placed at {at:?}, before it, so a copy would have them \
             in another order than they're shown",
            selecting.drawn.last().map(|before| before.at)
        );
        selecting.drawn.push(Drawn { text, at, bounds, area, leaves });
    }

    /// What's selected of the text at `at`, whose leaves say `leaves`, by leaf.
    pub(crate) fn selected_in<'a>(
        &self,
        at: TextAt,
        leaves: impl IntoIterator<Item = &'a str> + 'a,
    ) -> Vec<(u64, Range<usize>)> {
        let Some((start, end)) = self.ordered() else { return Vec::new() };
        selected(start, end, at, leaves).filter(|(_, range)| !range.is_empty()).collect()
    }

    /// Where the texts from one end of the selection to the other were last drawn, from the first to the last, which
    /// are kept for a copy, as the one a press was on is, with nothing selected yet, for a shift-click to extend from.
    /// A text whose place another has taken is there until it's drawn again.
    pub(crate) fn held(&self) -> Option<RangeInclusive<TextAt>> {
        let selecting = self.inner.borrow();
        let (a, b) = selecting.ends.as_ref()?;
        let (a, b) = (self.texts.last_drawn_at(&a.text)?, self.texts.last_drawn_at(&b.text)?);
        Some(a.min(b)..=a.max(b))
    }

    /// What a click on a link in a text does: opens it, as the kit does, unless something's selected, which the kit's
    /// text view doesn't open one for either. A left press lets go of what was selected, so that's when the press, or
    /// a drag from it, selected something.
    pub(crate) fn on_link(&self) -> impl Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + Send + Sync + 'static {
        let any = self.inner.borrow().any.clone();
        move |url, click, _, cx| {
            let opens = match click {
                ClickEvent::Mouse(click) => matches!(click.up.button, MouseButton::Left | MouseButton::Middle),
                ClickEvent::Keyboard(_) => true,
                ClickEvent::Touch(tap) => !tap.long_press,
            };
            if opens && !any.load(Ordering::Relaxed) {
                cx.open_url(url);
            }
        }
    }

    /// The button's come up, or the list it was pressed in has gone, so the drag's over, and what it selected stays.
    pub(crate) fn release(&self) {
        self.inner.borrow_mut().drag = None;
    }

    /// Moves the far end of what's being dragged to the place under the pointer, or the nearest in the window to it
    /// if it's past the window's edge, as the texts drawn in this frame have it. Says whether that changed what's
    /// selected.
    pub(crate) fn follow(&self, bounds: Bounds<Pixels>) -> bool {
        let (pointer, unit, chosen) = {
            let selecting = self.inner.borrow();
            let Some(drag) = selecting.drag.as_ref() else { return false };
            (drag.pointer, drag.unit, drag.chosen.clone())
        };
        let Some(chosen_at) = self.spots(&chosen) else { return false };
        let inside = point(pointer.x, pointer.y.clamp(bounds.top() + px(1.), bounds.bottom() - px(1.)));
        let under = spot_at(&self.inner.borrow().drawn, inside).map(|(spot, text)| (spot, text.clone()));
        let Some((spot, text)) = under else { return false };
        let (from, to) = self.around(spot, &text, unit);
        let (start, end) = chosen;
        let ends = if spot < chosen_at.0 {
            (end, End::at(text, from))
        } else if to < chosen_at.1 {
            (start, end)
        } else {
            (start, End::at(text, to))
        };
        let mut selecting = self.inner.borrow_mut();
        let changed = selecting.ends.as_ref() != Some(&ends);
        selecting.set(Some(ends));
        changed
    }

    fn is_dragging(&self) -> bool {
        self.inner.borrow().drag.is_some()
    }

    /// A press of the button at `position`, the `clicks`th in a row. On a text, it starts a selection there, or with
    /// `extend`, moves the far end of the one there is to there, and says so. Anywhere else, nothing's selected.
    fn press(&self, position: Point<Pixels>, clicks: usize, extend: bool) -> bool {
        let mut selecting = self.inner.borrow_mut();
        let on = text_under(&selecting.drawn, position);
        let (on_text, area) = (on.is_some(), on.and_then(|drawn| drawn.area.clone()));
        let Some((spot, text)) = spot_at(&selecting.drawn, position).filter(|_| on_text) else {
            selecting.set(None);
            selecting.drag = None;
            return false;
        };
        let text = text.clone();
        let unit = match clicks {
            0 | 1 => Unit::Character,
            2 => Unit::Word,
            _ => Unit::Paragraph,
        };
        // Extending from where what's selected starts, if its text still has a place, and otherwise starting afresh.
        let anchor = (selecting.ends.as_ref().map(|(anchor, _)| anchor.clone()))
            .filter(|anchor| extend && unit == Unit::Character && self.texts.place_of(&anchor.text).is_some());
        let chosen = if let Some(anchor) = anchor {
            selecting.set(Some((anchor.clone(), End::at(text, spot))));
            (anchor.clone(), anchor)
        } else {
            let (from, to) = self.around(spot, &text, unit);
            let chosen = (End::at(text.clone(), from), End::at(text, to));
            selecting.set(Some(chosen.clone()));
            chosen
        };
        let (scrolling, area_scrolling) = (ScrollingOn::default(), ScrollingOn::default());
        selecting.drag = Some(Drag { pointer: position, unit, chosen, scrolling, area, area_scrolling });
        true
    }

    /// The pointer's moved to `position` with the button held, over a list in `bounds`.
    fn drag_to(&self, position: Point<Pixels>, bounds: Bounds<Pixels>) {
        if let Some(drag) = self.inner.borrow_mut().drag.as_mut() {
            drag.pointer = position;
        }
        self.follow(bounds);
    }

    /// Selects all of the texts the chat holds that `crate::texts` draws, from the start of the first to the end of the
    /// last, and lets go of what the kit has selected, so a copy is of this. With none, it's for whatever else would
    /// take it.
    fn select_all(&self, window: &mut Window, cx: &mut App) {
        let Some(ends) = self.whole() else {
            cx.propagate();
            return;
        };
        TextSelection::clear(window, cx);
        let mut selecting = self.inner.borrow_mut();
        selecting.set(Some(ends));
        selecting.drag = None;
        window.refresh();
    }

    /// What a copy holds with everything selected, by the ends [`Selection::select_all`] selects and the copy
    /// [`Selection::text`] makes, for a test of the order the chat's texts are in.
    #[cfg(feature = "testing")]
    pub(crate) fn all_as_copied(&self) -> String {
        self.whole().and_then(|ends| self.spots(&ends)).map_or_default(|(start, end)| self.between(start, end))
    }

    /// What a copy would hold now, for a test of what a drag selects as it goes.
    #[cfg(feature = "testing")]
    pub(crate) fn as_copied(&self) -> String {
        self.text()
    }

    /// The ends of everything: the start of the first text that's shown and the end of the last, if there are any.
    fn whole(&self) -> Option<(End, End)> {
        let (first, last) = self.texts.ends()?;
        Some((End { text: first, leaf: 0, byte: 0 }, End { text: last, leaf: u64::MAX, byte: usize::MAX }))
    }

    /// What a copy holds of the texts from `start` to `end`.
    fn between(&self, start: Spot, end: Spot) -> String {
        copied(start, end, &self.texts.between(start.at, end.at))
    }

    /// Copies what's selected, drawn or not. With nothing selected, a copy is the kit's, of what it has selected.
    fn copy(&self, cx: &mut App) {
        let text = self.text();
        if text.is_empty() {
            cx.propagate();
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// What's selected, as text.
    fn text(&self) -> String {
        self.ordered().map_or_default(|(start, end)| self.between(start, end))
    }

    /// Its ends, first first, where they are now, if anything's selected.
    fn ordered(&self) -> Option<(Spot, Spot)> {
        let (a, b) = self.spots(self.inner.borrow().ends.as_ref()?)?;
        (a != b).then(|| if a <= b { (a, b) } else { (b, a) })
    }

    /// Where `ends` are now, if their texts are kept.
    fn spots(&self, (a, b): &(End, End)) -> Option<(Spot, Spot)> {
        let spot = |end: &End| Some(Spot { at: self.texts.place_of(&end.text)?, leaf: end.leaf, byte: end.byte });
        Some((spot(a)?, spot(b)?))
    }

    /// The `unit` that `spot`, in the text known as `text`, is in, as two places.
    fn around(&self, spot: Spot, text: &TextKey, unit: Unit) -> (Spot, Spot) {
        let leaves = self.texts.leaves_of(text);
        let range = leaves.as_ref().and_then(|leaves| {
            let text = leaves.leaf(spot.leaf)?;
            match (unit, leaves) {
                (Unit::Character, _) => None,
                (Unit::Word, _) => Some(word_around(text, spot.byte)),
                // A leaf of Markdown is a paragraph, a heading, a cell or a code block.
                (Unit::Paragraph, Leaves::Markdown(_)) => Some(0..text.len()),
                (Unit::Paragraph, Leaves::Plain(_)) => Some(line_around(text, spot.byte)),
            }
        });
        let range = range.unwrap_or(spot.byte..spot.byte);
        (Spot { byte: range.start, ..spot }, Spot { byte: range.end, ..spot })
    }
}
