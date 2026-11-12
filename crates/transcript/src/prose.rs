//! The agent's recorded words, drawn by the transcript from the kit's blocks rather than by the kit's text view, so
//! what's selected of them, and what's found in them, is the transcript's to keep while their rows are out of sight.
//! `docs/ui.md` has why.
//!
//! The kit parses the Markdown and draws each block as its text view would, through its `editor_host`. Each
//! paragraph, heading, code block and table cell is a leaf, keyed by its place in the text. The kit says where it drew
//! each leaf, so the transcript can find the character under the pointer, and paints the ranges it's given behind it,
//! which is how what's selected and what's found are drawn. `crate::selection` has the rest.
//!
//! A text is parsed the first time its row is drawn, which is soon after the row loads, since the list measures
//! loaded rows out of sight. What it shows is kept while its row is loaded, or it's selected, for a copy. The blocks
//! it's drawn from are kept for a long one, and for a short one only while it's among those drawn most recently, as
//! [`BLOCKS_KEPT`] says, and a short one is parsed again when it's next drawn. One over [`AT_ONCE`] is parsed on
//! another thread, and draws nothing until it's in, when its row is measured again. The text the agent is still
//! writing is drawn by the kit's text view, which `crate::long_texts` keeps. Once it's recorded, its row goes on
//! drawing that view until the record's own parse is in, so it doesn't go blank between.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;

use client_core::session_log::{Row, SessionLog};
use gpui_kit::{AnyElement, App, ImageSource, IntoElement, SharedUri, Task, WeakEntity};
use protocol::model::Seq;

use crate::find::{FindLook, TextId, view_id};
use crate::images_apart::images_apart;
use crate::selection::Selection;
use crate::transcript::Transcript;
use element::{Drawing, ProseElement};
pub(crate) use parse::{ATOM, Parsed, leaves};

mod element;
mod parse;

/// The longest text that's parsed where it's drawn. Parsing takes about 0.2 ms a KB on the main thread, so this
/// takes under a millisecond, and a longer one is parsed on another thread.
const AT_ONCE: usize = 4 * 1024;

/// How long the short texts whose blocks are kept can come to, in bytes of what they say, but for those drawn in this
/// frame. A text's blocks, with the kit's layout of them once they're drawn, take about 20 bytes for each of its own,
/// so this is about 2.5 MB. `docs/performance.md` has how that was measured.
const BLOCKS_KEPT: usize = 128 * 1024;
/// How many short texts are parsed between each look at whether too many blocks are kept.
const PARSES_A_LOOK: u32 = 8;

/// Where a text is in the chat: its event, and its place among the event's blocks. Ordered as the chat is, which
/// takes each event to draw its texts in its own row, in the order of its blocks, as the agent's replies do. A text
/// drawn elsewhere, such as in a call's row, or in another order, needs a place of its own to be ordered by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TextAt {
    pub(crate) seq: Seq,
    pub(crate) block: usize,
}

/// The texts drawn this way, parsed, by where they are in the chat. A copy reads the texts between a selection's ends
/// from here, drawn or not. Cheap to clone.
#[derive(Clone)]
pub(crate) struct Prose(Rc<RefCell<Store>>);

struct Store {
    /// Told when a text parsed on another thread is in, to measure its row again.
    transcript: WeakEntity<Transcript>,
    texts: BTreeMap<TextAt, Text>,
    /// How many frames the list has drawn, which says which text was drawn longest ago.
    frame: u64,
    /// How many short texts have been parsed since the blocks kept were last looked at.
    parses: u32,
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

/// One of the agent's recorded texts: where it is in the chat, what find knows it by, and what it says.
pub(crate) struct Written<'a> {
    pub(crate) at: TextAt,
    pub(crate) id: TextId,
    pub(crate) text: &'a str,
}

impl Prose {
    pub(crate) fn new(transcript: WeakEntity<Transcript>) -> Self {
        Self(Rc::new(RefCell::new(Store { transcript, texts: BTreeMap::new(), frame: 0, parses: 0 })))
    }

    /// The list has started drawing a frame, so the texts drawn in the last can be put away, if any were parsed.
    pub(crate) fn new_frame(&self) {
        let mut texts = self.0.borrow_mut();
        texts.frame += 1;
        if texts.parses > 0 {
            texts.parses = 0;
            texts.keep_fewer_blocks();
        }
    }

    /// Draws `written`, with its images found by `images`, and what's found in it as `find` says. Until a long text
    /// is parsed, it's what `until_parsed` draws, or nothing.
    pub(crate) fn draw(
        &self,
        selection: &Selection,
        find: &FindLook,
        written: &Written<'_>,
        images: impl Fn(&SharedUri) -> ImageSource + Send + Sync + 'static,
        until_parsed: impl FnOnce() -> Option<AnyElement>,
    ) -> AnyElement {
        let source = images_apart(written.text);
        let (text, marks) = match self.parsed(written.at, &source) {
            Some(parsed) => {
                let marks = find.marks_in_leaves(&written.id, &parsed.leaves);
                (Drawing::Parsed(parsed), marks)
            }
            None => (Drawing::Unparsed { source: source.into_owned(), meanwhile: until_parsed() }, Vec::new()),
        };
        let (prose, selection) = (self.clone(), selection.clone());
        let images = Some(Box::new(images) as element::Images);
        ProseElement { id: view_id(&written.id), at: written.at, text, marks, images, prose, selection }
            .into_any_element()
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
        texts.parses += 1;
        if texts.parses >= PARSES_A_LOOK {
            texts.parses = 0;
            texts.keep_fewer_blocks();
        }
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

    /// What the texts from `from` to `to`, both in, show, leaf by leaf, in order, as far as they've been parsed.
    pub(crate) fn between(&self, from: TextAt, to: TextAt) -> Vec<(TextAt, Arc<[String]>)> {
        let texts = self.0.borrow();
        texts.texts.range(from..=to).filter_map(|(at, text)| Some((*at, text.leaves()?))).collect()
    }

    /// What the text at `at` shows, leaf by leaf, if it's been parsed.
    pub(crate) fn leaves_of(&self, at: TextAt) -> Option<Arc<[String]>> {
        self.0.borrow().texts.get(&at)?.leaves()
    }

    /// Where the first and last texts are, if there are any.
    pub(crate) fn ends(&self) -> Option<(TextAt, TextAt)> {
        let texts = self.0.borrow();
        Some((*texts.texts.first_key_value()?.0, *texts.texts.last_key_value()?.0))
    }

    /// Lets go of each text whose event isn't loaded, but for those in `selected`, so a copy still has them.
    pub(crate) fn let_go(&self, log: &SessionLog, selected: Option<&RangeInclusive<TextAt>>) {
        let loaded = |at: &TextAt| log.row_of(at.seq).is_some_and(|row| matches!(log.row(row), Row::Event(_)));
        let selected = |at: &TextAt| selected.is_some_and(|selected| selected.contains(at));
        self.0.borrow_mut().texts.retain(|at, _| loaded(at) || selected(at));
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

impl Store {
    /// Puts away the blocks of the short texts drawn longest ago, so those kept come to no more than [`BLOCKS_KEPT`],
    /// but for those drawn in this frame, which its rows hold anyway. A long text keeps its blocks, as parsing it
    /// again on another thread would leave it blank meanwhile.
    fn keep_fewer_blocks(&mut self) {
        let frame = self.frame;
        let mut short: Vec<(u64, TextAt, usize)> = (self.texts.iter())
            .filter_map(|(at, text)| match text {
                Text::Parsed { parsed, drawn } if parsed.source.len() <= AT_ONCE && *drawn < frame => {
                    Some((*drawn, *at, parsed.source.len()))
                }
                Text::Parsed { .. } | Text::Read { .. } | Text::Parsing { .. } => None,
            })
            .collect();
        let mut kept: usize = short.iter().map(|(_, _, len)| len).sum();
        short.sort_unstable_by_key(|(drawn, ..)| *drawn);
        for (_, at, len) in short {
            if kept <= BLOCKS_KEPT {
                break;
            }
            if let Some(Text::Parsed { parsed, .. }) = self.texts.remove(&at) {
                self.texts.insert(at, Text::Read { leaves: parsed.leaves.clone() });
            }
            kept -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::WeakEntity;
    use protocol::event::{ContentBlock, EventBody};

    use super::{BLOCKS_KEPT, PARSES_A_LOOK, Prose, Text, TextAt};
    use crate::fixtures::{event, log};

    fn at(seq: u64) -> TextAt {
        TextAt { seq, block: 0 }
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

    /// Has `prose` draw `texts` texts of `words`, each in a frame of its own, and says which then have blocks. They're
    /// drawn from the last in the chat to the first, so going by when they were drawn isn't going by where they are.
    fn drawn_from_the_last(prose: &Prose, words: &str, texts: u64) -> Vec<u64> {
        for seq in (0..texts).rev() {
            prose.new_frame();
            prose.parsed(at(seq), words);
        }
        let texts_now = prose.0.borrow();
        (0..texts).filter(|seq| matches!(texts_now.texts.get(&at(*seq)), Some(Text::Parsed { .. }))).collect()
    }

    #[test]
    fn the_blocks_of_the_short_texts_drawn_longest_ago_are_put_away_but_not_what_they_show() {
        let prose = Prose::new(WeakEntity::new_invalid());
        let words = "word ".repeat(200);
        let texts = u64::try_from(2 * BLOCKS_KEPT / words.len()).unwrap();

        let with_blocks = drawn_from_the_last(&prose, &words, texts);
        prose.new_frame();
        prose.parsed(at(texts - 1), &words);

        assert!(with_blocks.contains(&0), "the last drawn keeps them");
        assert!(!with_blocks.contains(&(texts - 1)), "the first drawn doesn't");
        let most = BLOCKS_KEPT / words.len() + usize::try_from(PARSES_A_LOOK).unwrap();
        assert!(with_blocks.len() <= most, "{} of {texts} kept them", with_blocks.len());
        assert!((0..texts).all(|seq| prose.leaves_of(at(seq)).is_some()), "each still says what it shows");
        let again = matches!(prose.0.borrow().texts.get(&at(texts - 1)), Some(Text::Parsed { .. }));
        assert!(again, "and one drawn again is parsed again");
    }

    #[test]
    fn nothing_drawn_in_this_frame_is_put_away() {
        let prose = Prose::new(WeakEntity::new_invalid());
        let words = "word ".repeat(200);
        let texts = u64::try_from(2 * BLOCKS_KEPT / words.len()).unwrap();

        prose.new_frame();
        for seq in 0..texts {
            prose.parsed(at(seq), &words);
        }

        let texts_now = prose.0.borrow();
        assert!(texts_now.texts.values().all(|text| matches!(text, Text::Parsed { .. })));
    }
}
