//! The chat's texts whose selection the transcript keeps, rather than the window, so what's selected of them, and
//! what's found in them, outlasts their rows being drawn. They're the chat's Markdown, drawn from the kit's blocks
//! rather than by the kit's text view: the agent's replies and thoughts, from when it starts writing them, prompts,
//! from when they're sent, a plan to approve, a compaction's summary, the prose a call shows of its own, such as a
//! subagent's report, what a call's row nests, such as a subagent's words, and what the subagent tab says its subagent
//! was asked and reported, above and below its rows. They're also the plain text a call shows, such as a command and
//! what it printed, which `plain` draws. `docs/ui.md` has why.
//!
//! The kit parses the Markdown and draws each block as its text view would, through its `editor_host`. Each
//! paragraph, heading, code block and table cell is a leaf, keyed by its place in the text. The kit says where it drew
//! each leaf, so the transcript can find the character under the pointer, and paints the ranges it's given behind it,
//! which is how what's selected and what's found are drawn. `crate::selection` has the rest. Plain text isn't parsed.
//! It's one leaf, what it says as find searches it, however it's laid out, such as a line at a time, and it's kept, as
//! what it says, as any other text is.
//!
//! A text is parsed the first time its row is drawn, which is soon after the row loads, since the list measures
//! loaded rows out of sight. What it shows is kept while its row is loaded, or it's selected, for a copy, and the
//! subagent tab's head and foot while the tab is open. The blocks it's drawn from are kept for a long one, and for a
//! short one only while it's among those drawn most recently, as `budget` says, and a short one is parsed again when
//! it's next drawn. A text at the end of the chat that goes on from what it was parsed as is parsed again from near
//! its end, keeping the blocks before, and is parsed whole once it's drawn anywhere else, since the two can differ.
//! When more than [`AT_ONCE`] is to be parsed, it's parsed on another thread, as `parsing` says, and draws what it was
//! parsed as before until it's in, or nothing if it wasn't, and then its row is measured again. A text is kept by what
//! it's known by, which is the same from a reply's first word to its record, and from a prompt being sent to the row
//! where the agent took it, so it's drawn from what it was parsed as once it's moved there, as `moves` says, and
//! doesn't go blank between.
//!
//! A row draws its event's texts through [`Texts::drawing`], as the subagent tab draws its head and foot, which hides
//! each text it no longer draws, such as a thought that's been closed, so a copy has only what's shown. A hidden text
//! is kept, as any other is, so it's drawn again at once when it's shown again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;

use client_core::session_log::{Row, SessionLog};
use gpui_kit::{AnyElement, App, ElementId, IntoElement, SharedString, Task};

use crate::find::{FindLook, TextId, view_id};
use crate::images_apart::images_apart;
use crate::linked_images::Images;
use crate::selection::Selection;
pub(crate) use at::{Among, Of, Place, TextAt};
use element::ProseElement;
use moves::Places;
use parse::ImagesAre;
pub(crate) use parse::{ATOM, Parsed, leaves};
pub(crate) use plain::Plain;

mod at;
mod budget;
mod element;
mod moves;
mod parse;
mod parsing;
mod plain;

/// The most of a text that's parsed where it's drawn: all of it, or what's parsed again of one that goes on. Parsing
/// takes about 0.1 ms a KB of prose on the main thread, and 0.35 ms a KB of a list, so this takes under 1.5 ms, and
/// more is parsed on another thread. `docs/performance.md` has how that was measured.
const AT_ONCE: usize = 4 * 1024;

/// What's being drawn, such as an event's row: the texts it's drawn so far, and how many of them each of its blocks
/// nests.
struct Drawing {
    of: Of,
    these: HashSet<TextAt>,
    nested: HashMap<usize, usize>,
}

/// What a text is known by: what its element is known by, which stays the same as it moves, as a reply does from being
/// written to its record, since `find::view_id` makes it the same, or for plain text, what the row names it by.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextKey(ElementId);

/// The chat's texts, as far as each is known, by what each is known by, and where each is in the chat now. A
/// copy reads the texts between a selection's ends from here, drawn or not. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Texts(Rc<RefCell<Store>>);

/// What's told when a text parsed on another thread is in, to measure the row that draws what it says again.
type Redrawn = Rc<dyn Fn(Of, &mut App)>;

struct Store {
    redrawn: Redrawn,
    texts: HashMap<TextKey, Text>,
    /// Where each text is now, and where and when it was last drawn, as `moves` says.
    places: Places,
    /// The texts kept whose rows don't draw them now, such as a thought that's been closed, which a copy leaves out.
    hidden: HashSet<TextKey>,
    /// How many frames the list has drawn, which says which text was drawn longest ago.
    frame: u64,
    /// How many short texts have been parsed since the blocks kept were last looked at.
    parses: u32,
    /// What's being drawn, if anything is.
    in_row: Option<Drawing>,
}

enum Text {
    /// Parsed, with the blocks it's drawn from.
    Parsed { parsed: Rc<Parsed> },
    /// A short text not drawn lately, with what it shows, for a copy, and not its blocks, which it's parsed for again
    /// when it's next drawn.
    Read { leaves: Arc<[String]> },
    /// Being parsed from `source` on another thread, by a task that's dropped, and stops, with this. It's drawn as what
    /// it was parsed as before, `was`, if it was, until it's in.
    Parsing { source: String, was: Option<Rc<Parsed>>, _task: Task<()> },
    /// Plain text, which isn't parsed: what it says, as find searches it.
    Plain { said: SharedString },
}

impl Text {
    /// What it's being parsed from on another thread, if it is.
    fn parsing(&self) -> Option<&str> {
        match self {
            Text::Parsing { source, .. } => Some(source),
            Text::Parsed { .. } | Text::Read { .. } | Text::Plain { .. } => None,
        }
    }

    /// What it shows, as far as it's been parsed.
    fn leaves(&self) -> Option<Leaves> {
        match self {
            Text::Parsed { parsed, .. } | Text::Parsing { was: Some(parsed), .. } => {
                Some(Leaves::Markdown(parsed.leaves.clone()))
            }
            Text::Read { leaves } => Some(Leaves::Markdown(leaves.clone())),
            Text::Plain { said } => Some(Leaves::Plain(said.clone())),
            Text::Parsing { was: None, .. } => None,
        }
    }

    /// What it was last parsed as, which a text that goes on from it is parsed again from the end of.
    fn was(&self) -> Option<Rc<Parsed>> {
        match self {
            Text::Parsed { parsed, .. } => Some(parsed.clone()),
            Text::Parsing { was, .. } => was.clone(),
            Text::Read { .. } | Text::Plain { .. } => None,
        }
    }
}

/// What a text shows, leaf by leaf, which the bytes of a place in it count in. Cheap to clone.
#[derive(Clone)]
pub(crate) enum Leaves {
    /// Markdown's: each a paragraph, heading, code block or table cell, where each image the text shows is an
    /// [`ATOM`], which a copy leaves out.
    Markdown(Arc<[String]>),
    /// Plain text's, which is one leaf: what it says, as find searches it, however it's laid out, so a place in it is
    /// the same place to find and to the selection. It has no images, so an [`ATOM`] in it is a character it says.
    Plain(SharedString),
}

impl Leaves {
    /// What the leaf `leaf` shows, if there is one.
    pub(crate) fn leaf(&self, leaf: u64) -> Option<&str> {
        let leaf = usize::try_from(leaf).ok()?;
        match self {
            Leaves::Markdown(leaves) => leaves.get(leaf).map(String::as_str),
            Leaves::Plain(said) => (leaf == 0).then_some(said.as_ref()),
        }
    }

    /// What each leaf shows, in order.
    pub(crate) fn each(&self) -> impl Iterator<Item = &str> {
        let (markdown, plain) = match self {
            Leaves::Markdown(leaves) => (Some(leaves.iter().map(String::as_str)), None),
            Leaves::Plain(said) => (None, Some(said.as_ref())),
        };
        markdown.into_iter().flatten().chain(plain)
    }
}

/// One of the chat's Markdown texts, recorded or not yet: where it is in the chat, what its element is known by
/// from one frame to the next, what find knows it by and how what it finds is drawn, if find looks in it, what it says,
/// and the area that scrolls it, if it's in one.
pub(crate) struct Written<'a> {
    pub(crate) at: Place,
    pub(crate) element: ElementId,
    pub(crate) found: Option<(&'a TextId, &'a FindLook)>,
    pub(crate) text: &'a str,
    pub(crate) area: Option<ElementId>,
}

impl<'a> Written<'a> {
    /// A text find looks in, which it knows as `id`, with what it finds drawn as `find` says.
    pub(crate) fn found(at: impl Into<Place>, id: &'a TextId, find: &'a FindLook, text: &'a str) -> Self {
        Self { at: at.into(), element: view_id(id), found: Some((id, find)), text, area: None }
    }

    /// A text find doesn't look in, such as a plan, whose element is known as `element`.
    pub(crate) fn unfound(at: TextAt, element: ElementId, text: &'a str) -> Self {
        Self { at: Place::At(at), element, found: None, text, area: None }
    }

    /// In the area `area`, which scrolls it, and scrolls on under a drag that starts in it.
    pub(crate) fn in_area(mut self, area: ElementId) -> Self {
        self.area = Some(area);
        self
    }
}

impl Store {
    /// Notes that what's being drawn draws the text known as `known` at `place`, and says where that is: where it
    /// says, or for the next text a block nests, after those of them drawn so far in this drawing of the row.
    fn noted(&mut self, known: &TextKey, place: Place) -> TextAt {
        let nested = self.in_row.as_ref().map(|mine| &mine.nested);
        let at = match place {
            Place::At(at) => at,
            Place::NextNested { seq, block } => {
                let drawn = nested.and_then(|nested| nested.get(&block)).copied().unwrap_or(0);
                TextAt { of: Of::Event(seq), block, among: Among::Nested(drawn) }
            }
        };
        let drawing = self.in_row.as_mut().map(|mine| {
            mine.these.insert(at);
            if matches!(place, Place::NextNested { .. }) {
                *mine.nested.entry(at.block).or_default() += 1;
            }
            mine.of
        });
        debug_assert_eq!(drawing, Some(at.of), "a text drawn outside what draws it is never hidden once it isn't");
        self.placed(known, at);
        at
    }
}

impl Texts {
    /// The texts of a transcript, which `redrawn` tells when one parsed on another thread is in, so the row drawing
    /// what it says is measured again.
    pub(crate) fn new(redrawn: impl Fn(Of, &mut App) + 'static) -> Self {
        Self(Rc::new(RefCell::new(Store {
            redrawn: Rc::new(redrawn),
            texts: HashMap::new(),
            places: Places::default(),
            hidden: HashSet::new(),
            frame: 0,
            parses: 0,
            in_row: None,
        })))
    }

    /// The list has started drawing a frame, so the texts drawn in the last can be put away, if any were parsed.
    pub(crate) fn new_frame(&self) {
        self.0.borrow_mut().frame_started();
    }

    /// Draws what `of` says with `draw`, such as the row of an event, and hides each of its texts it no longer draws,
    /// such as one in a fold that's been closed, or for one a call nests, takes its place away, so a copy has only
    /// what's shown. Only its own texts are looked at, so
    /// a row costs the same however many the chat holds.
    pub(crate) fn drawing<T>(&self, of: Of, draw: impl FnOnce() -> T) -> T {
        let mine = Drawing { of, these: HashSet::new(), nested: HashMap::new() };
        let outer = self.0.borrow_mut().in_row.replace(mine);
        let drawn = draw();
        let mut store = self.0.borrow_mut();
        let these = std::mem::replace(&mut store.in_row, outer).map(|mine| mine.these).unwrap_or_default();
        let Store { places, hidden, .. } = &mut *store;
        let mut left = Vec::new();
        for (at, id) in places.within(TextAt::first_of(of)..).take_while(|(at, _)| at.of == of) {
            if these.contains(at) {
                hidden.remove(id);
            } else if matches!(at.among, Among::Nested(_)) {
                // Its place is only its turn among those drawn, which it has none of now, and another may have a
                // later one that came before it.
                left.push(*at);
            } else {
                hidden.insert(id.clone());
            }
        }
        for at in left {
            places.leave(&at);
        }
        drawn
    }

    /// Draws `written`, with its images drawn as `images` says, or as the kit draws them if there's none. Until a long
    /// text is parsed, it's what it was parsed as before, or nothing if it wasn't.
    pub(crate) fn draw(&self, selection: &Selection, written: &Written<'_>, images: Option<Images>) -> AnyElement {
        let known = TextKey(written.element.clone());
        let at = self.0.borrow_mut().noted(&known, written.at);
        let source = images_apart(written.text);
        let whose = ImagesAre::of(&source, images.is_some());
        let (parsed, apart) = self.parsed(&known, &source, at.of, whose);
        let marks = (parsed.as_ref().zip(written.found))
            .map_or_else(Vec::new, |(parsed, (id, find))| find.marks_in_leaves(id, &parsed.leaves));
        let (texts, selection) = (self.clone(), selection.clone());
        let area = written.area.clone();
        ProseElement { known, at, area, text: parsed, apart, marks, images, texts, selection }.into_any_element()
    }

    /// What the texts from `from` to `to`, both in, show, leaf by leaf, in order, as far as they've been parsed, but
    /// for those hidden.
    pub(crate) fn between(&self, from: TextAt, to: TextAt) -> Vec<(TextAt, Leaves)> {
        let store = self.0.borrow();
        let shown = store.places.within(from..=to).filter(|(_, id)| !store.hidden.contains(id));
        shown.filter_map(|(at, id)| Some((*at, store.texts.get(id)?.leaves()?))).collect()
    }

    /// What the text known as `id` shows, leaf by leaf, if that's known: if it's plain text, or it's been parsed.
    pub(crate) fn leaves_of(&self, id: &TextKey) -> Option<Leaves> {
        self.0.borrow().texts.get(id)?.leaves()
    }

    /// The first and last texts that aren't hidden, in the chat's order, if there are any.
    pub(crate) fn ends(&self) -> Option<(TextKey, TextKey)> {
        let store = self.0.borrow();
        let mut shown = store.places.within(..).map(|(_, id)| id).filter(|id| !store.hidden.contains(*id));
        let first = shown.next()?.clone();
        let last = shown.next_back().cloned().unwrap_or_else(|| first.clone());
        Some((first, last))
    }

    /// Lets go of each text last drawn in an event's row that isn't loaded, but for those in `selected`, so a copy
    /// still has them. The subagent tab's head and foot are its subagent's, and kept with the tab. The end of the chat
    /// is always there, and keeps the few texts it's drawn most recently and doesn't now, as `moves` says.
    pub(crate) fn let_go(&self, log: &SessionLog, selected: Option<&RangeInclusive<TextAt>>) {
        let loaded = |at: &TextAt| match at.of {
            Of::Event(seq) => log.row_of(seq).is_some_and(|row| matches!(log.row(row), Row::Event(_))),
            Of::Head | Of::Tail | Of::Foot => true,
        };
        let held = |at: &TextAt| selected.is_some_and(|selected| selected.contains(at));
        let mut store = self.0.borrow_mut();
        let Store { texts, places, hidden, .. } = &mut *store;
        places.retain(|at| loaded(at) || held(at));
        places.keep_few_left_at_the_end(hidden, held);
        texts.retain(|id, _| places.last_at(id).is_some());
        hidden.retain(|id| places.last_at(id).is_some());
    }

    /// Whether every text drawn has been parsed, so each row has the height it will keep.
    #[cfg(feature = "testing")]
    pub(crate) fn all_parsed(&self) -> bool {
        self.0.borrow().texts.values().all(|text| !matches!(text, Text::Parsing { .. }))
    }

    /// What each text that keeps its blocks was parsed from.
    #[cfg(feature = "testing")]
    pub(crate) fn sources(&self) -> Vec<String> {
        let store = self.0.borrow();
        (store.texts.values())
            .filter_map(|text| match text {
                Text::Parsed { parsed, .. } => Some(parsed.source.clone()),
                Text::Read { .. } | Text::Parsing { .. } | Text::Plain { .. } => None,
            })
            .collect()
    }
}

#[cfg(test)]
impl Texts {
    /// Texts in no transcript, for a test, so nothing's told when one parsed on another thread is in.
    pub(crate) fn alone() -> Self {
        Self::new(|_, _| {})
    }
}

#[cfg(test)]
mod tests;
