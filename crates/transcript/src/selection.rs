//! What's selected of the chat's recorded texts, which the transcript holds rather than the window, so it outlasts
//! the rows it's in being drawn. The kit's selection is the window's, over the texts painted in a frame, and lets go
//! of what's selected once a text holding either end of it isn't painted, which in a list that draws only what's in
//! sight is as soon as that text's row leaves the window.
//!
//! Each end is a place in a text: which text, which of its leaves, and a byte in that leaf. Each frame, every text
//! that's drawn says where its leaves are, so a point in the window can be turned into a place. A press on a text
//! starts a selection, a drag moves its far end, and with the pointer past the top or bottom of the list, the list
//! scrolls on, faster the further past it is, for as long as the button is held. A copy reads the texts between the
//! ends as they were parsed, drawn or not, which is each the chat has loaded, and each it loaded as the selection was
//! made, since what's selected is kept. `crate::prose` draws them.
//!
//! It's the mouse's: a press, a drag and a release. The kit's selection also takes a touch screen's long press and
//! handles, which a transcript on a phone will need here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Range, RangeInclusive};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use gpui_kit::base::TextSelection;
use gpui_kit::base::input::{Copy, SelectAll};
use gpui_kit::base::text::editor_host::LeafGeometry;
use gpui_kit::{
    App, Bounds, ClickEvent, ClipboardItem, Div, ElementId, FocusHandle, InteractiveElement as _, MouseButton, Pixels,
    Point, SharedString, Window, point, px,
};

use crate::prose::{Prose, TextAt};

mod places;
mod pointer;

use places::{copied, leaf_text, selected, spot_at, word_around};

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

/// What a press selects, and a drag from it goes by: a character, a word for a double click, or a whole paragraph,
/// heading, cell or code block for a triple.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Unit {
    Character,
    Word,
    Leaf,
}

/// The selection in one transcript. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Selection {
    inner: Rc<RefCell<Selecting>>,
    prose: Prose,
    /// The transcript's, which a press on its text takes, so a copy and a select all come to it.
    focus: FocusHandle,
}

#[derive(Default)]
struct Selecting {
    /// Where it started and where it's got to, in either order. Nothing's selected while they're the same.
    ends: Option<(Spot, Spot)>,
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
    chosen: (Spot, Spot),
    /// When the list last moved under the pointer past its edge.
    moved: Option<Instant>,
    /// The area that scrolls the text the press was on, if it's in one.
    area: Option<ElementId>,
    /// When that area last moved under the pointer past its edge.
    area_moved: Option<Instant>,
}

/// A text drawn in this frame, what shows of it, the area that scrolls it, if it's in one, and where each of its
/// leaves is, by key.
struct Drawn {
    at: TextAt,
    bounds: Bounds<Pixels>,
    area: Option<ElementId>,
    leaves: Vec<(u64, LeafGeometry)>,
}

impl Selecting {
    fn set(&mut self, ends: Option<(Spot, Spot)>) {
        self.ends = ends;
        self.any.store(ends.is_some_and(|(a, b)| a != b), Ordering::Relaxed);
    }
}

impl Selection {
    pub(crate) fn new(prose: Prose, focus: FocusHandle) -> Self {
        Self { inner: Rc::default(), prose, focus }
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
        self.prose.new_frame();
    }

    /// Notes that the text at `at` shows in `bounds`, in `area` if one scrolls it, with its leaves where `leaves`
    /// says. A leaf that doesn't show, such as one an area that scrolls the text has moved out of sight, can't be
    /// pressed on or dragged to.
    pub(crate) fn drawn(
        &self,
        at: TextAt,
        bounds: Bounds<Pixels>,
        area: Option<ElementId>,
        leaves: HashMap<u64, LeafGeometry>,
    ) {
        let showing = leaves.into_iter().filter(|(_, geometry)| geometry.bounds().intersects(&bounds));
        let mut leaves: Vec<(u64, LeafGeometry)> = showing.collect();
        leaves.sort_by_key(|(key, _)| *key);
        self.inner.borrow_mut().drawn.push(Drawn { at, bounds, area, leaves });
    }

    /// What's selected of the text at `at`, whose leaves are `leaves`, by leaf.
    pub(crate) fn selected_in(&self, at: TextAt, leaves: &[String]) -> Vec<(u64, Range<usize>)> {
        let Some((start, end)) = self.ordered() else { return Vec::new() };
        selected(start, end, at, leaves).filter(|(_, range)| !range.is_empty()).collect()
    }

    /// The texts anything's selected in, if it is, from the first to the last.
    pub(crate) fn texts(&self) -> Option<RangeInclusive<TextAt>> {
        self.ordered().map(|(start, end)| start.at..=end.at)
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
            (drag.pointer, drag.unit, drag.chosen)
        };
        let inside = point(pointer.x, pointer.y.clamp(bounds.top() + px(1.), bounds.bottom() - px(1.)));
        let Some(spot) = spot_at(&self.inner.borrow().drawn, inside) else { return false };
        let (from, to) = self.around(spot, unit);
        let ends = if spot < chosen.0 { (chosen.1, from) } else { (chosen.0, to.max(chosen.1)) };
        let mut selecting = self.inner.borrow_mut();
        let changed = selecting.ends != Some(ends);
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
        let on = selecting.drawn.iter().find(|drawn| drawn.bounds.contains(&position));
        let (on_text, area) = (on.is_some(), on.and_then(|drawn| drawn.area.clone()));
        let Some(spot) = spot_at(&selecting.drawn, position).filter(|_| on_text) else {
            selecting.set(None);
            selecting.drag = None;
            return false;
        };
        let unit = match clicks {
            0 | 1 => Unit::Character,
            2 => Unit::Word,
            _ => Unit::Leaf,
        };
        let chosen = match selecting.ends {
            Some((anchor, _)) if extend && unit == Unit::Character => {
                selecting.set(Some((anchor, spot)));
                (anchor, anchor)
            }
            Some(_) | None => {
                let chosen = self.around(spot, unit);
                selecting.set(Some(chosen));
                chosen
            }
        };
        selecting.drag = Some(Drag { pointer: position, unit, chosen, moved: None, area, area_moved: None });
        true
    }

    /// The pointer's moved to `position` with the button held, over a list in `bounds`.
    fn drag_to(&self, position: Point<Pixels>, bounds: Bounds<Pixels>) {
        if let Some(drag) = self.inner.borrow_mut().drag.as_mut() {
            drag.pointer = position;
        }
        self.follow(bounds);
    }

    /// Selects all of the texts the chat holds that `crate::prose` draws, from the start of the first to the end of the
    /// last, and lets go of what the kit has selected, so a copy is of this. With none, it's for whatever else would
    /// take it.
    fn select_all(&self, window: &mut Window, cx: &mut App) {
        let Some((first, last)) = self.prose.ends() else {
            cx.propagate();
            return;
        };
        TextSelection::clear(window, cx);
        let mut selecting = self.inner.borrow_mut();
        let end = Spot { at: last, leaf: u64::MAX, byte: usize::MAX };
        selecting.set(Some((Spot { at: first, leaf: 0, byte: 0 }, end)));
        selecting.drag = None;
        window.refresh();
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
        let Some((start, end)) = self.ordered() else { return String::new() };
        copied(start, end, &self.prose.between(start.at, end.at))
    }

    /// Its ends, first first, if anything's selected.
    fn ordered(&self) -> Option<(Spot, Spot)> {
        let (a, b) = self.inner.borrow().ends?;
        (a != b).then(|| if a <= b { (a, b) } else { (b, a) })
    }

    /// The `unit` that `spot` is in, as two places.
    fn around(&self, spot: Spot, unit: Unit) -> (Spot, Spot) {
        let leaves = self.prose.leaves_of(spot.at);
        let text = leaves.as_deref().and_then(|leaves| leaf_text(leaves, spot.leaf));
        let range = match (unit, text) {
            (Unit::Character, _) | (Unit::Word | Unit::Leaf, None) => spot.byte..spot.byte,
            (Unit::Word, Some(text)) => word_around(text, spot.byte),
            (Unit::Leaf, Some(text)) => 0..text.len(),
        };
        (Spot { byte: range.start, ..spot }, Spot { byte: range.end, ..spot })
    }
}
