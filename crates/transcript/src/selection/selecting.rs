use super::keys::Keyed;
use super::{geometry, places};
use crate::reveal::Reveal;
use crate::texts::{Leaves, TextAt, TextKey, Texts};
use geometry::Geometry;
use gpui_kit::base::TextSelection;
use gpui_kit::base::input::{Copy, SelectAll};
use gpui_kit::{
    App, Bounds, ClickEvent, ClipboardItem, Div, ElementId, FocusHandle, InteractiveElement as _, MouseButton, Pixels,
    Point, SharedString, Window,
};
use look::motion::ScrollingOn;
use places::{copied, line_around, selected, word_around};
use std::cell::RefCell;
use std::ops::{Range, RangeInclusive};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Where the keys that copy what's selected, select everything and move an end of what's selected are bound, which is
/// the transcript once a press in it has focused it.
pub const KEY_CONTEXT: &str = "Transcript";

/// A place in a text, between two characters, in the chat's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Spot {
    pub(super) at: TextAt,
    pub(super) leaf: u64,
    pub(super) byte: usize,
}

/// An end of what's selected: a place in the text known as `text`, wherever that is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct End {
    pub(super) text: TextKey,
    pub(super) leaf: u64,
    pub(super) byte: usize,
}

impl End {
    /// The place `spot` names, in the text known as `text`, which is there now.
    pub(super) fn at(text: TextKey, spot: Spot) -> Self {
        Self { text, leaf: spot.leaf, byte: spot.byte }
    }
}

/// What a press selects, and a drag from it goes by: a character, a word for a double click, or for a triple, a whole
/// paragraph, heading, cell or code block of Markdown, or a line of plain text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Unit {
    Character,
    Word,
    Paragraph,
}

/// The selection in one transcript. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Selection {
    pub(super) inner: Rc<RefCell<Selecting>>,
    pub(super) texts: Texts,
    /// The transcript's, which a press on its text takes, so a copy and a select all come to it.
    pub(super) focus: FocusHandle,
    /// What's to be brought into view in the view the texts are drawn in, which what scrolls them scrolls to, as
    /// `Scrolled` has it.
    pub(super) reveal: Reveal,
}

#[derive(Default)]
pub(super) struct Selecting {
    /// Where it started and where it's got to, in either order. Nothing's selected while they're the same.
    pub(super) ends: Option<(End, End)>,
    /// Whether anything is, which a link asks as the button comes up over it, from a handler the kit has be `Send`.
    pub(super) any: Arc<AtomicBool>,
    pub(super) drag: Option<Drag>,
    /// The texts drawn in this frame, in the order they were drawn, which is the chat's.
    pub(super) drawn: Vec<Drawn>,
    /// What the keys that move an end have left to do.
    pub(super) keyed: Keyed,
}

pub(super) struct Drag {
    pub(super) pointer: Point<Pixels>,
    pub(super) unit: Unit,
    /// What the press chose, which stays selected however the drag goes.
    pub(super) chosen: (End, End),
    /// The list scrolling on under the pointer past its edge.
    pub(super) scrolling: ScrollingOn,
    /// The area that scrolls the text the press was on, if it's in one.
    pub(super) area: Option<ElementId>,
    /// That area scrolling on under the pointer past its edge.
    pub(super) area_scrolling: ScrollingOn,
}

/// A text drawn in this frame, what it's known by and where it is, what shows of it, the area that scrolls it, if it's
/// in one, and where each of its leaves was laid out, by key, whether it shows or not.
pub(super) struct Drawn {
    pub(super) text: TextKey,
    pub(super) at: TextAt,
    pub(super) bounds: Bounds<Pixels>,
    pub(super) area: Option<ElementId>,
    pub(super) leaves: Vec<(u64, Geometry)>,
}

impl Drawn {
    /// Its leaves that show, such as those an area that scrolls the text hasn't moved out of sight, which are the ones
    /// a press can be on.
    pub(super) fn showing(&self) -> impl Iterator<Item = &(u64, Geometry)> {
        self.leaves.iter().filter(|(_, geometry)| geometry.shows(self.bounds))
    }
}

impl Selecting {
    pub(super) fn set(&mut self, ends: Option<(End, End)>) {
        self.any.store(ends.as_ref().is_some_and(|(a, b)| a != b), Ordering::Relaxed);
        self.ends = ends;
    }
}

impl Selection {
    pub(crate) fn new(texts: Texts, focus: FocusHandle) -> Self {
        Self { inner: Rc::default(), texts, focus, reveal: Reveal::default() }
    }

    /// What's to be brought into view in the view the texts are drawn in: the end of what's selected that a key moved,
    /// or the match Find is showing.
    pub(crate) fn reveal(&self) -> &Reveal {
        &self.reveal
    }

    /// `chat`, the transcript's, with its focus, and the keys that copy what's selected, select everything, and move
    /// the end of what's selected that moved last, as `keys` says. They're the actions a text box takes for the same
    /// keys. GPUI gives focus to what tracks it on a press in it that nothing else takes, so a press anywhere in the
    /// chat gives it the focus, rather than the window, which tracks it everywhere else.
    pub(crate) fn with_keys(&self, chat: Div) -> Div {
        let (copying, selecting) = (self.clone(), self.clone());
        let chat = (chat.key_context(KEY_CONTEXT).track_focus(&self.focus))
            .on_action(move |_: &Copy, _, cx| copying.copy(cx))
            .on_action(move |_: &SelectAll, window, cx| selecting.select_all(window, cx));
        self.with_moving_keys(chat)
    }

    /// The list has started drawing a frame: forgets the texts drawn in the last, before this one's are, and tells the
    /// texts, which keep the blocks of those drawn lately.
    pub(crate) fn new_frame(&self) {
        self.inner.borrow_mut().drawn.clear();
        self.texts.new_frame();
    }

    /// Notes that the text known as `text`, at `at`, shows in `bounds`, in `area` if one scrolls it, with its leaves
    /// where `leaves` says. A leaf that doesn't show, such as one an area that scrolls the text has moved out of sight,
    /// can't be pressed on or dragged to, but a key can move the end of what's selected into it, which the area then
    /// scrolls into view.
    pub(crate) fn drawn(
        &self,
        text: TextKey,
        at: TextAt,
        bounds: Bounds<Pixels>,
        area: Option<ElementId>,
        leaves: impl IntoIterator<Item = (u64, Geometry)>,
    ) {
        let mut leaves: Vec<(u64, Geometry)> = leaves.into_iter().collect();
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

    /// Selects all of the texts the chat holds that `crate::texts` draws, from the start of the first to the end of the
    /// last, and lets go of what the kit has selected, so a copy is of this. With none, it's for whatever else would
    /// take it.
    pub(super) fn select_all(&self, window: &mut Window, cx: &mut App) {
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

    /// Where each leaf drawn in the last frame was laid out, in the order they were drawn.
    #[cfg(feature = "testing")]
    pub(crate) fn drawn_leaf_bounds(&self) -> Vec<Bounds<Pixels>> {
        self.inner.borrow().drawn.iter().flat_map(|drawn| drawn.leaves.iter().map(|(_, leaf)| leaf.bounds())).collect()
    }

    /// The ends of everything: the start of the first text that's shown and the end of the last, if there are any.
    pub(super) fn whole(&self) -> Option<(End, End)> {
        let (first, last) = self.texts.ends()?;
        Some((End { text: first, leaf: 0, byte: 0 }, End { text: last, leaf: u64::MAX, byte: usize::MAX }))
    }

    /// What a copy holds of the texts from `start` to `end`.
    pub(super) fn between(&self, start: Spot, end: Spot) -> String {
        copied(start, end, &self.texts.between(start.at, end.at))
    }

    /// Copies what's selected, drawn or not. With nothing selected, a copy is the kit's, of what it has selected.
    pub(crate) fn copy(&self, cx: &mut App) {
        let text = self.text();
        if text.is_empty() {
            cx.propagate();
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// What's selected, as text.
    pub(super) fn text(&self) -> String {
        self.ordered().map_or_default(|(start, end)| self.between(start, end))
    }

    /// Its ends, first first, where they are now, if anything's selected.
    pub(super) fn ordered(&self) -> Option<(Spot, Spot)> {
        let (a, b) = self.spots(self.inner.borrow().ends.as_ref()?)?;
        (a != b).then(|| if a <= b { (a, b) } else { (b, a) })
    }

    /// Where `ends` are now, if their texts are kept.
    pub(super) fn spots(&self, (a, b): &(End, End)) -> Option<(Spot, Spot)> {
        let spot = |end: &End| Some(Spot { at: self.texts.place_of(&end.text)?, leaf: end.leaf, byte: end.byte });
        Some((spot(a)?, spot(b)?))
    }

    /// The `unit` that `spot`, in the text known as `text`, is in, as two places.
    pub(super) fn around(&self, spot: Spot, text: &TextKey, unit: Unit) -> (Spot, Spot) {
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
