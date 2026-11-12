//! The recorded texts that sit in a row of their own, drawn by the transcript from the kit's blocks rather than by the
//! kit's text view, so what's selected of them, and what's found in them, is the transcript's to keep while their rows
//! are out of sight. They're the agent's replies and thoughts, prompts, a plan to approve, a compaction's summary,
//! and the prose a call in a row of its own shows of its own, such as a subagent's report. `docs/ui.md` has why.
//!
//! The kit parses the Markdown and draws each block as its text view would, through its `editor_host`. Each
//! paragraph, heading, code block and table cell is a leaf, keyed by its place in the text. The kit says where it drew
//! each leaf, so the transcript can find the character under the pointer, and paints the ranges it's given behind it,
//! which is how what's selected and what's found are drawn. `crate::selection` has the rest.
//!
//! A text is parsed the first time its row is drawn, which is soon after the row loads, since the list measures
//! loaded rows out of sight. What it shows is kept while its row is loaded, or it's selected, for a copy. The blocks
//! it's drawn from are kept for a long one, and for a short one only while it's among those drawn most recently, as
//! `budget` says, and a short one is parsed again when it's next drawn. One over [`AT_ONCE`] is parsed on
//! another thread, and draws nothing until it's in, when its row is measured again. The text the agent is still
//! writing, and a prompt being sent, are drawn by the kit's text view, which `crate::long_texts` keeps. Once it's
//! recorded, its row goes on drawing that view until the record's own parse is in, so it doesn't go blank between.
//!
//! A row draws its event's texts through [`Prose::drawing_event`], which hides each it no longer draws, such as a
//! thought that's been closed, so a copy has only what's shown. A hidden text is kept, as any other is, so it's drawn
//! again at once when it's shown again.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;

use client_core::session_log::{Row, SessionLog};
use gpui_kit::{AnyElement, App, ElementId, ImageSource, IntoElement, SharedUri, Task, WeakEntity};
use protocol::model::Seq;

use crate::find::{FindLook, Part, TextId, view_id};
use crate::images_apart::images_apart;
use crate::selection::Selection;
use crate::transcript::Transcript;
use element::{Drawing, ProseElement};
pub(crate) use parse::{ATOM, Parsed, leaves};

mod budget;
mod element;
mod parse;

/// The longest text that's parsed where it's drawn. Parsing takes about 0.2 ms a KB on the main thread, so this
/// takes under a millisecond, and a longer one is parsed on another thread.
const AT_ONCE: usize = 4 * 1024;

/// Where a text is in the chat: the event whose row draws it, which of the event's blocks it's drawn in, and where it
/// is among the texts that block draws. Ordered as the chat is, since a row draws its blocks in order, and a block its
/// texts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TextAt {
    pub(crate) seq: Seq,
    pub(crate) block: usize,
    pub(crate) among: Among,
}

impl TextAt {
    /// The only text the `block`th block of the event at `seq` draws, such as a reply's.
    pub(crate) fn only(seq: Seq, block: usize) -> Self {
        Self { seq, block, among: Among::Only }
    }

    /// The first place in the row of the event at `seq`.
    fn first_in(seq: Seq) -> Self {
        Self::only(seq, 0)
    }
}

/// Where a text is among those one block draws, in the order the block draws them. Each is known by what it is, rather
/// than by how many texts the block drew before it, so it keeps its place while the block draws more or fewer, such as
/// a call that's opened or closed, and no other text takes it. Ordered as declared, so `Only` is the first place in a
/// block, which [`TextAt::first_in`] counts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Among {
    /// The only text the block draws, such as a reply's.
    Only,
    /// One of a call's own texts, such as what a subagent was asked, or its report, in the order of their parts.
    Call(Part),
}

/// The texts drawn this way, parsed, by where they are in the chat. A copy reads the texts between a selection's ends
/// from here, drawn or not. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Prose(Rc<RefCell<Store>>);

struct Store {
    /// Told when a text parsed on another thread is in, to measure its row again.
    transcript: WeakEntity<Transcript>,
    texts: BTreeMap<TextAt, Text>,
    /// The texts kept whose rows don't draw them now, such as a thought that's been closed, which a copy leaves out.
    hidden: BTreeSet<TextAt>,
    /// How many frames the list has drawn, which says which text was drawn longest ago.
    frame: u64,
    /// How many short texts have been parsed since the blocks kept were last looked at.
    parses: u32,
    /// The texts drawn so far in the row of the event being drawn, if one is.
    in_row: Option<Vec<TextAt>>,
}

enum Text {
    /// Parsed, with the blocks it's drawn from, and last drawn in the `drawn`th frame.
    Parsed { parsed: Rc<Parsed>, drawn: u64 },
    /// A short text not drawn lately, with what it shows, for a copy, and not its blocks, which it's parsed for again
    /// when it's next drawn.
    Read { leaves: Arc<[String]> },
    /// Being parsed on another thread, by a task that's dropped, and stops, with this.
    Parsing { source: String, _task: Task<()> },
}

impl Text {
    /// Whether it's being parsed from `source` on another thread.
    fn is_parsing(&self, source: &str) -> bool {
        matches!(self, Text::Parsing { source: parsing, .. } if parsing == source)
    }

    fn leaves(&self) -> Option<Arc<[String]>> {
        match self {
            Text::Parsed { parsed, .. } => Some(parsed.leaves.clone()),
            Text::Read { leaves } => Some(leaves.clone()),
            Text::Parsing { .. } => None,
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
    pub(crate) fn new(transcript: WeakEntity<Transcript>) -> Self {
        Self(Rc::new(RefCell::new(Store {
            transcript,
            texts: BTreeMap::new(),
            hidden: BTreeSet::new(),
            frame: 0,
            parses: 0,
            in_row: None,
        })))
    }

    /// The list has started drawing a frame, so the texts drawn in the last can be put away, if any were parsed.
    pub(crate) fn new_frame(&self) {
        self.0.borrow_mut().frame_started();
    }

    /// Draws the row of the event at `seq` with `draw`, and hides each of the event's texts it no longer draws, such
    /// as one in a fold that's been closed, so a copy has only what's shown. Only the event's own texts are looked at,
    /// so a row costs the same however many the chat holds.
    pub(crate) fn drawing_event<T>(&self, seq: Seq, draw: impl FnOnce() -> T) -> T {
        let outer = self.0.borrow_mut().in_row.replace(Vec::new());
        let drawn = draw();
        let mut texts = self.0.borrow_mut();
        let these = std::mem::replace(&mut texts.in_row, outer).unwrap_or_default();
        let Store { texts, hidden, .. } = &mut *texts;
        let in_row = texts.range(TextAt::first_in(seq)..).take_while(|(at, _)| at.seq == seq);
        for at in in_row.map(|(at, _)| *at) {
            if these.contains(&at) {
                hidden.remove(&at);
            } else {
                hidden.insert(at);
            }
        }
        drawn
    }

    /// Draws `written`, with its images found by `images`. Until a long text is parsed, it's what `until_parsed`
    /// draws, or nothing.
    pub(crate) fn draw(
        &self,
        selection: &Selection,
        written: &Written<'_>,
        images: Option<Images>,
        until_parsed: impl FnOnce() -> Option<AnyElement>,
    ) -> AnyElement {
        let in_row = self.0.borrow_mut().in_row.as_mut().map(|in_row| in_row.push(written.at)).is_some();
        debug_assert!(in_row, "a text was drawn outside an event's row, so it's never hidden once it isn't drawn");
        let source = images_apart(written.text);
        let (text, marks) = match self.parsed(written.at, &source) {
            Some(parsed) => {
                let marks = written.found.map_or_else(Vec::new, |(id, find)| find.marks_in_leaves(id, &parsed.leaves));
                (Drawing::Parsed(parsed), marks)
            }
            None => (Drawing::Unparsed { source: source.into_owned(), meanwhile: until_parsed() }, Vec::new()),
        };
        let (prose, selection) = (self.clone(), selection.clone());
        let (id, at, area) = (written.element.clone(), written.at, written.area.clone());
        ProseElement { id, at, area, text, marks, images, prose, selection }.into_any_element()
    }

    /// `source`, parsed, as the text at `at`, to draw, if it has been, or it's short enough to parse now.
    fn parsed(&self, at: TextAt, source: &str) -> Option<Rc<Parsed>> {
        let mut texts = self.0.borrow_mut();
        let now = texts.frame;
        match texts.texts.get_mut(&at) {
            Some(Text::Parsed { parsed, drawn }) if parsed.source == source => {
                *drawn = now;
                return Some(parsed.clone());
            }
            Some(Text::Parsing { source: parsing, .. }) if parsing == source => return None,
            Some(_) | None => {}
        }
        if source.len() > AT_ONCE {
            return None;
        }
        let parsed = Rc::new(parse::parse(source.to_owned()));
        texts.texts.insert(at, Text::Parsed { parsed: parsed.clone(), drawn: now });
        texts.parsed_one();
        Some(parsed)
    }

    /// Parses `source` on another thread as the text at `at`, unless it's already being parsed, and measures its row
    /// again once it's in. It says what it said, so there's nothing new to find in it.
    fn parse_apart(&self, at: TextAt, source: &str, cx: &mut App) {
        let mut texts = self.0.borrow_mut();
        if texts.texts.get(&at).is_some_and(|text| text.is_parsing(source)) {
            return;
        }
        let (prose, transcript, given) = (Rc::downgrade(&self.0), texts.transcript.clone(), source.to_owned());
        let parsing = cx.spawn(async move |cx| {
            let parsed = cx.background_executor().spawn(async move { parse::parse(given) }).await;
            let Some(prose) = prose.upgrade() else { return };
            // What it replaces holds this task, which goes once it's done.
            let _was = {
                let mut texts = prose.borrow_mut();
                if !texts.texts.get(&at).is_some_and(|text| text.is_parsing(&parsed.source)) {
                    return;
                }
                let drawn = texts.frame;
                texts.texts.insert(at, Text::Parsed { parsed: Rc::new(parsed), drawn })
            };
            let _ = transcript.update(cx, |transcript, cx| transcript.redrawn(at.seq, cx));
        });
        texts.texts.insert(at, Text::Parsing { source: source.to_owned(), _task: parsing });
    }

    /// What the texts from `from` to `to`, both in, show, leaf by leaf, in order, as far as they've been parsed, but
    /// for those hidden.
    pub(crate) fn between(&self, from: TextAt, to: TextAt) -> Vec<(TextAt, Arc<[String]>)> {
        let texts = self.0.borrow();
        let shown = texts.texts.range(from..=to).filter(|(at, _)| !texts.hidden.contains(at));
        shown.filter_map(|(at, text)| Some((*at, text.leaves()?))).collect()
    }

    /// What the text at `at` shows, leaf by leaf, if it's been parsed.
    pub(crate) fn leaves_of(&self, at: TextAt) -> Option<Arc<[String]>> {
        self.0.borrow().texts.get(&at)?.leaves()
    }

    /// Where the first and last texts that aren't hidden are, if there are any.
    pub(crate) fn ends(&self) -> Option<(TextAt, TextAt)> {
        let texts = self.0.borrow();
        let mut shown = texts.texts.keys().filter(|at| !texts.hidden.contains(at));
        let first = *shown.next()?;
        Some((first, shown.next_back().copied().unwrap_or(first)))
    }

    /// Lets go of each text whose event isn't loaded, but for those in `selected`, so a copy still has them.
    pub(crate) fn let_go(&self, log: &SessionLog, selected: Option<&RangeInclusive<TextAt>>) {
        let loaded = |at: &TextAt| log.row_of(at.seq).is_some_and(|row| matches!(log.row(row), Row::Event(_)));
        let selected = |at: &TextAt| selected.is_some_and(|selected| selected.contains(at));
        let mut texts = self.0.borrow_mut();
        let Store { texts, hidden, .. } = &mut *texts;
        texts.retain(|at, _| loaded(at) || selected(at));
        hidden.retain(|at| texts.contains_key(at));
    }

    /// Whether every text drawn has been parsed, so each row has the height it will keep.
    #[cfg(feature = "testing")]
    pub(crate) fn all_parsed(&self) -> bool {
        self.0.borrow().texts.values().all(|text| text.leaves().is_some())
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
mod tests {
    use gpui_kit::WeakEntity;
    use protocol::event::{ContentBlock, EventBody};

    use super::{Prose, TextAt};
    use crate::fixtures::{event, log};

    fn at(seq: u64) -> TextAt {
        TextAt::only(seq, 0)
    }

    #[test]
    fn a_text_whose_event_is_let_go_of_is_kept_while_any_of_it_is_selected() {
        let prose = Prose::new(WeakEntity::new_invalid());
        for seq in 1..=3 {
            prose.parsed(at(seq), "words");
        }
        let blocks = vec![ContentBlock::Text { text: "words".into(), images: Vec::new() }];
        let body =
            EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None };
        let held = log([event(3, body)]);

        prose.let_go(&held, Some(&(at(2)..=at(3))));
        let kept = [1, 2, 3].map(|seq| prose.leaves_of(at(seq)).is_some());
        prose.let_go(&held, None);

        assert_eq!(kept, [false, true, true], "what's loaded, and what's selected");
        assert!(prose.leaves_of(at(2)).is_none(), "until it isn't");
    }
}
