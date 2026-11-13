//! The recorded texts that sit in a row of their own, drawn by the transcript from the kit's blocks rather than by the
//! kit's text view, so what's selected of them, and what's found in them, is the transcript's to keep while their rows
//! are out of sight. They're the agent's replies and thoughts, prompts, a plan to approve, a compaction's summary,
//! the prose a call in a row of its own shows of its own, such as a subagent's report, and what the subagent tab says
//! its subagent was asked and reported, above and below its rows. `docs/ui.md` has why.
//!
//! The kit parses the Markdown and draws each block as its text view would, through its `editor_host`. Each
//! paragraph, heading, code block and table cell is a leaf, keyed by its place in the text. The kit says where it drew
//! each leaf, so the transcript can find the character under the pointer, and paints the ranges it's given behind it,
//! which is how what's selected and what's found are drawn. `crate::selection` has the rest.
//!
//! A text is parsed the first time its row is drawn, which is soon after the row loads, since the list measures
//! loaded rows out of sight. What it shows is kept while its row is loaded, or it's selected, for a copy, and the
//! subagent tab's head and foot while the tab is open. The blocks it's drawn from are kept for a long one, and for a
//! short one only while it's among those drawn most recently, as `budget` says, and a short one is parsed again when
//! it's next drawn. A text that goes on from what it was parsed as is parsed again from near its end, keeping the
//! blocks before. When more than [`AT_ONCE`] is to be parsed, it's parsed on another thread, as `parsing` says, and
//! draws what it was parsed as before until it's in, if it goes on from that, or nothing, and then its row is measured
//! again. The text the agent is still writing, and a prompt being sent, are drawn by the kit's text view, which
//! `crate::long_texts` keeps. Once it's recorded, its row goes on drawing that view until the record's own parse is
//! in, so it doesn't go blank between.
//!
//! A row draws its event's texts through [`Prose::drawing`], as the subagent tab draws its head and foot, which hides
//! each text it no longer draws, such as a thought that's been closed, so a copy has only what's shown. A hidden text
//! is kept, as any other is, so it's drawn again at once when it's shown again.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;

use client_core::session_log::{Row, SessionLog};
use gpui_kit::{AnyElement, App, ElementId, ImageSource, IntoElement, SharedUri, Task};
use protocol::model::Seq;

use crate::find::{FindLook, Part, TextId, view_id};
use crate::images_apart::images_apart;
use crate::selection::Selection;
use element::{Drawing, ProseElement};
use moves::Places;
pub(crate) use parse::{ATOM, Parsed, leaves};

mod budget;
mod element;
mod moves;
mod parse;
mod parsing;

/// The most of a text that's parsed where it's drawn: all of it, or what's parsed again of one that goes on. Parsing
/// takes about 0.1 ms a KB of prose on the main thread, and 0.35 ms a KB of a list, so this takes under 1.5 ms, and
/// more is parsed on another thread. `docs/performance.md` has how that was measured.
const AT_ONCE: usize = 4 * 1024;

/// Where a text is in the chat: what draws it, which of its blocks it's drawn in, and where it is among the texts that
/// block draws. Ordered as the chat is, since a row draws its blocks in order, and a block its texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TextAt {
    pub(crate) of: Of,
    pub(crate) block: usize,
    pub(crate) among: Among,
}

impl TextAt {
    /// The only text the `block`th block of the event at `seq` draws, such as a reply's.
    pub(crate) fn only(seq: Seq, block: usize) -> Self {
        Self { of: Of::Event(seq), block, among: Among::Only }
    }

    /// The first place in what `of` draws, which is the only text's when it draws one, as the subagent tab's head and
    /// foot do.
    pub(crate) fn first_of(of: Of) -> Self {
        Self { of, block: 0, among: Among::Only }
    }

    /// The event its images are found by, if it's in one's row.
    pub(crate) fn seq(self) -> Option<Seq> {
        match self.of {
            Of::Event(seq) => Some(seq),
            Of::Head | Of::Foot => None,
        }
    }
}

/// What draws a text, in the chat's order: the subagent tab's head, which says what its subagent was asked above the
/// tab's first row, an event's row, or the tab's foot, its report, below the last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Of {
    Head,
    Event(Seq),
    Foot,
}

/// Where a text is among those one block draws, in the order the block draws them. Each is known by what it is, rather
/// than by how many texts the block drew before it, so it keeps its place while the block draws more or fewer, such as
/// a call that's opened or closed, and no other text takes it. Ordered as declared, so `Only` is the first place in a
/// block, which [`TextAt::first_of`] counts on, as the first place in what draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Among {
    /// The only text the block draws, such as a reply's.
    Only,
    /// One of a call's own texts, such as what a subagent was asked, or its report, in the order of their parts.
    Call(Part),
}

/// What a text drawn this way is known by: what its element is known by, which stays the same as it moves, as a reply
/// does from being written to its record, since `find::view_id` makes it the same.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ProseId(ElementId);

/// The texts drawn this way, parsed, by what each is known by, and where each is in the chat now. A copy reads the
/// texts between a selection's ends from here, drawn or not. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Prose(Rc<RefCell<Store>>);

/// What's told when a text parsed on another thread is in, to measure the row that draws what it says again.
type Redrawn = Rc<dyn Fn(Of, &mut App)>;

struct Store {
    redrawn: Redrawn,
    texts: HashMap<ProseId, Text>,
    /// Where each text is now, and where and when it was last drawn, as `moves` says.
    places: Places,
    /// The texts kept whose rows don't draw them now, such as a thought that's been closed, which a copy leaves out.
    hidden: HashSet<ProseId>,
    /// How many frames the list has drawn, which says which text was drawn longest ago.
    frame: u64,
    /// How many short texts have been parsed since the blocks kept were last looked at.
    parses: u32,
    /// What's being drawn, if anything is, such as an event's row, and the texts it's drawn so far.
    in_row: Option<(Of, Vec<TextAt>)>,
}

enum Text {
    /// Parsed, with the blocks it's drawn from.
    Parsed { parsed: Rc<Parsed> },
    /// A short text not drawn lately, with what it shows, for a copy, and not its blocks, which it's parsed for again
    /// when it's next drawn.
    Read { leaves: Arc<[String]> },
    /// Being parsed from `source` on another thread, by a task that's dropped, and stops, with this. One that goes on
    /// from what it was parsed as is drawn as that, `was`, until it's in.
    Parsing { source: String, was: Option<Rc<Parsed>>, _task: Task<()> },
}

impl Text {
    /// What it's being parsed from on another thread, if it is.
    fn parsing(&self) -> Option<&str> {
        match self {
            Text::Parsing { source, .. } => Some(source),
            Text::Parsed { .. } | Text::Read { .. } => None,
        }
    }

    /// What it shows, as far as it's been parsed.
    fn leaves(&self) -> Option<Arc<[String]>> {
        match self {
            Text::Parsed { parsed, .. } | Text::Parsing { was: Some(parsed), .. } => Some(parsed.leaves.clone()),
            Text::Read { leaves } => Some(leaves.clone()),
            Text::Parsing { was: None, .. } => None,
        }
    }

    /// What it was last parsed as, which a text that goes on from it is parsed again from the end of.
    fn was(&self) -> Option<Rc<Parsed>> {
        match self {
            Text::Parsed { parsed, .. } => Some(parsed.clone()),
            Text::Parsing { was, .. } => was.clone(),
            Text::Read { .. } => None,
        }
    }
}

/// One of the chat's recorded texts: where it is in the chat, what its element is known by from one frame to the
/// next, what find knows it by and how what it finds is drawn, if find looks in it, what it says, and the area that
/// scrolls it, if it's in one.
pub(crate) struct Written<'a> {
    pub(crate) at: TextAt,
    pub(crate) element: ElementId,
    pub(crate) found: Option<(&'a TextId, &'a FindLook)>,
    pub(crate) text: &'a str,
    pub(crate) area: Option<ElementId>,
}

impl<'a> Written<'a> {
    /// A text find looks in, which it knows as `id`, with what it finds drawn as `find` says.
    pub(crate) fn found(at: TextAt, id: &'a TextId, find: &'a FindLook, text: &'a str) -> Self {
        Self { at, element: view_id(id), found: Some((id, find)), text, area: None }
    }

    /// A text find doesn't look in, such as a plan, whose element is known as `element`.
    pub(crate) fn unfound(at: TextAt, element: ElementId, text: &'a str) -> Self {
        Self { at, element, found: None, text, area: None }
    }

    /// In the area `area`, which scrolls it, and scrolls on under a drag that starts in it.
    pub(crate) fn in_area(mut self, area: ElementId) -> Self {
        self.area = Some(area);
        self
    }
}

/// Where the images a text points to are found, which the kit's own way does if there's none.
pub(crate) type Images = Box<dyn Fn(&SharedUri) -> ImageSource + Send + Sync>;

impl Prose {
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
    /// such as one in a fold that's been closed, so a copy has only what's shown. Only its own texts are looked at, so
    /// a row costs the same however many the chat holds.
    pub(crate) fn drawing<T>(&self, of: Of, draw: impl FnOnce() -> T) -> T {
        let outer = self.0.borrow_mut().in_row.replace((of, Vec::new()));
        let drawn = draw();
        let mut texts = self.0.borrow_mut();
        let these = std::mem::replace(&mut texts.in_row, outer).map(|(_, these)| these).unwrap_or_default();
        let Store { places, hidden, .. } = &mut *texts;
        for (at, id) in places.within(TextAt::first_of(of)..).take_while(|(at, _)| at.of == of) {
            if these.contains(at) {
                hidden.remove(id);
            } else {
                hidden.insert(id.clone());
            }
        }
        drawn
    }

    /// Draws `written`, with its images found by `images`. Until a long text is parsed, it's what it was parsed as
    /// before, if it goes on from that, or what `until_parsed` draws, or nothing.
    pub(crate) fn draw(
        &self,
        selection: &Selection,
        written: &Written<'_>,
        images: Option<Images>,
        until_parsed: impl FnOnce() -> Option<AnyElement>,
    ) -> AnyElement {
        let drawing = self.0.borrow_mut().in_row.as_mut().map(|(of, these)| {
            these.push(written.at);
            *of
        });
        debug_assert_eq!(
            drawing,
            Some(written.at.of),
            "a text drawn outside what draws it is never hidden once it isn't"
        );
        let known = ProseId(written.element.clone());
        self.0.borrow_mut().placed(&known, written.at);
        let source = images_apart(written.text);
        let (parsed, apart) = self.parsed(&known, &source);
        let (text, marks) = match parsed {
            Some(parsed) => {
                let marks = written.found.map_or_else(Vec::new, |(id, find)| find.marks_in_leaves(id, &parsed.leaves));
                (Drawing::Parsed(parsed), marks)
            }
            None => (Drawing::Meanwhile(until_parsed()), Vec::new()),
        };
        let (prose, selection) = (self.clone(), selection.clone());
        let (at, area) = (written.at, written.area.clone());
        ProseElement { known, at, area, text, apart, marks, images, prose, selection }.into_any_element()
    }

    /// What the texts from `from` to `to`, both in, show, leaf by leaf, in order, as far as they've been parsed, but
    /// for those hidden.
    pub(crate) fn between(&self, from: TextAt, to: TextAt) -> Vec<(TextAt, Arc<[String]>)> {
        let store = self.0.borrow();
        let shown = store.places.within(from..=to).filter(|(_, id)| !store.hidden.contains(id));
        shown.filter_map(|(at, id)| Some((*at, store.texts.get(id)?.leaves()?))).collect()
    }

    /// What the text known as `id` shows, leaf by leaf, if it's been parsed.
    pub(crate) fn leaves_of(&self, id: &ProseId) -> Option<Arc<[String]>> {
        self.0.borrow().texts.get(id)?.leaves()
    }

    /// The first and last texts that aren't hidden, in the chat's order, if there are any.
    pub(crate) fn ends(&self) -> Option<(ProseId, ProseId)> {
        let store = self.0.borrow();
        let mut shown = store.places.within(..).map(|(_, id)| id).filter(|id| !store.hidden.contains(*id));
        let first = shown.next()?.clone();
        let last = shown.next_back().cloned().unwrap_or_else(|| first.clone());
        Some((first, last))
    }

    /// Lets go of each text last drawn in an event's row that isn't loaded, but for those in `selected`, so a copy
    /// still has them. The subagent tab's head and foot are its subagent's, and kept with the tab.
    pub(crate) fn let_go(&self, log: &SessionLog, selected: Option<&RangeInclusive<TextAt>>) {
        let loaded = |at: &TextAt| match at.of {
            Of::Event(seq) => log.row_of(seq).is_some_and(|row| matches!(log.row(row), Row::Event(_))),
            Of::Head | Of::Foot => true,
        };
        let keep = |at: &TextAt| loaded(at) || selected.is_some_and(|selected| selected.contains(at));
        let mut store = self.0.borrow_mut();
        let Store { texts, places, hidden, .. } = &mut *store;
        places.retain(keep);
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
        let texts = self.0.borrow();
        (texts.texts.values())
            .filter_map(|text| match text {
                Text::Parsed { parsed, .. } => Some(parsed.source.clone()),
                Text::Read { .. } | Text::Parsing { .. } => None,
            })
            .collect()
    }
}

#[cfg(test)]
impl Prose {
    /// Texts in no transcript, for a test, so nothing's told when one parsed on another thread is in.
    pub(crate) fn alone() -> Self {
        Self::new(|_, _| {})
    }
}

#[cfg(test)]
mod tests {
    use protocol::event::{ContentBlock, EventBody};

    use super::{Of, Prose, ProseId, TextAt};
    use crate::fixtures::{event, log};

    fn at(seq: u64) -> TextAt {
        TextAt::only(seq, 0)
    }

    #[test]
    fn the_subagent_tab_s_head_comes_before_every_row_s_texts_and_its_foot_after() {
        let (head, foot) = (TextAt::first_of(Of::Head), TextAt::first_of(Of::Foot));
        let rows = [TextAt::only(0, 0), TextAt::only(u64::MAX, usize::MAX)];

        assert!(rows.iter().all(|row| head < *row && *row < foot), "{head:?} {rows:?} {foot:?}");
    }

    #[test]
    fn a_text_whose_event_is_let_go_of_is_kept_while_any_of_it_is_selected() {
        let prose = Prose::alone();
        for seq in 1..=3 {
            prose.drawn_at(at(seq), "words");
        }
        let blocks = vec![ContentBlock::Text { text: "words".into(), images: Vec::new() }];
        let body =
            EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None };
        let held = log([event(3, body)]);

        prose.let_go(&held, Some(&(at(2)..=at(3))));
        let kept = [1, 2, 3].map(|seq| prose.keeps(&ProseId::of(at(seq))));
        prose.let_go(&held, None);

        assert_eq!(kept, [false, true, true], "what's loaded, and what's selected");
        assert!(!prose.keeps(&ProseId::of(at(2))), "until it isn't");
    }

    #[test]
    fn the_subagent_tab_s_head_and_foot_are_kept_whatever_the_log_lets_go_of() {
        let prose = Prose::alone();
        let (head, foot) = (TextAt::first_of(Of::Head), TextAt::first_of(Of::Foot));
        prose.drawn_at(head, "What it was asked");
        prose.drawn_at(foot, "What it reported");

        prose.let_go(&log([]), None);

        assert!(prose.keeps(&ProseId::of(head)) && prose.keeps(&ProseId::of(foot)));
    }
}
