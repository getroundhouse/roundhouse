//! What drawing needs to show what's being found: where the needle is in each text a row draws, and which match is the
//! one being shown. Each text is known by the item it's part of and which of its texts it is, the same way whether
//! it's drawn or only counted, so a match found while counting is the one drawn.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{ElementId, HighlightStyle, Hsla};
use protocol::find::Needle;

use super::reveal::Reveal;
use crate::item_key::ItemKey;

/// Which of an item's texts one is, such as a call's output, or the third link a search found. Ordered as declared,
/// which orders the texts a call draws of its own after what it nests, in `crate::texts`, if it draws more than one
/// there, so those are declared in the order it draws them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Part {
    /// What the person wrote.
    Prompt,
    /// What the agent wrote.
    Text,
    Thinking,
    /// What a call's line says, when it's the agent's words, such as what it said a command is for.
    Title,
    /// The code on a call's line, such as a search's pattern.
    Pill,
    /// The name over the lines a call opens to, such as a file's path.
    Label,
    Command,
    Input,
    Output,
    /// All of what went wrong, under a call's line.
    Error,
    /// What the person said as they denied a call, under its line.
    Said,
    /// A file's lines.
    File,
    Notice,
    /// A closed call's summary of what it opens to. Its matches are marked but not counted, and never the one shown,
    /// since what it sums up is counted, and shown open.
    Summary,
    /// What a subagent was asked.
    Asked,
    /// A subagent's report, when its own words don't say it.
    Report,
    /// What the advisor said.
    Advice,
    /// What the agent asked of a page, and what was made of it.
    Ask,
    Answer,
    /// What a search of the web made of the pages it found.
    Findings,
    /// What a search found, other than lines in files.
    Found,
    /// The title of the `n`th page a search of the web found.
    Link(u32),
    /// The path of the `n`th file a search found lines in, and the lines.
    FoundPath(u32),
    FoundLines(u32),
    /// What went wrong in a turn that failed.
    TurnError,
    /// Why a request to the model failed, in the agent's CLI's words.
    FailedRequest,
    /// What the agent last printed before it stopped unexpectedly.
    Exit,
}

impl Part {
    /// The `n`th of several, such as a link, by `nth`.
    pub(crate) fn nth(nth: fn(u32) -> Self, index: usize) -> Self {
        nth(u32::try_from(index).unwrap_or(u32::MAX))
    }

    /// The ID of this text of the item `key`, where it's drawn in a part with an ID of its own, so it keeps its place
    /// when it scrolls.
    pub(crate) fn element(self, key: &ItemKey) -> ElementId {
        key.element(&self.name())
    }

    /// What its ID is made from.
    pub(crate) fn name(self) -> Cow<'static, str> {
        Cow::Borrowed(match self {
            Part::Prompt | Part::Asked => "prompt",
            Part::Text => "text",
            Part::Thinking => "thinking-text",
            Part::Title => "title",
            Part::Pill => "pill",
            Part::Label => "label",
            Part::Command => "command",
            Part::Input => "input",
            Part::Output => "output",
            Part::Error => "error",
            Part::Said => "said",
            Part::File => "file",
            Part::Notice => "notice",
            Part::Summary | Part::Findings => "summary",
            Part::Report => "report",
            Part::Advice => "advice",
            Part::Ask => "ask",
            Part::Answer => "answer",
            Part::Found => "found",
            Part::Link(n) => return Cow::Owned(format!("link-{n}")),
            Part::FoundPath(n) => return Cow::Owned(format!("found-path-{n}")),
            Part::FoundLines(n) => return Cow::Owned(format!("found-lines-{n}")),
            Part::TurnError => "turn-error",
            Part::FailedRequest => "failed-request",
            Part::Exit => "exit",
        })
    }
}

/// A text a row draws, by the item it's part of and which of its texts it is.
pub(crate) type TextId = (ItemKey, Part);

/// The ID of the element the Markdown text `id` is drawn in, which `crate::texts` knows it by, from its first word to
/// its record. A prompt, the agent's words and its thinking are only their element, and a report is drawn as the
/// agent's words are. Prose in a block is an element inside the part.
pub(crate) fn view_id((key, part): &TextId) -> ElementId {
    if matches!(part, Part::Prompt | Part::Text | Part::Thinking) {
        part.element(key)
    } else if *part == Part::Report {
        key.element("text")
    } else {
        key.element(&format!("{}-text", part.name()))
    }
}

/// A match: the text it's in, and which of the matches there it is, counting from 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Spot {
    pub(crate) text: TextId,
    pub(crate) nth: usize,
}

/// What's being found, as drawing sees it. A new one is made whenever what's looked for or the match being shown
/// changes, so a row drawn with one is drawn as it was then.
#[derive(Default)]
pub(crate) struct FindLook {
    needle: Option<Needle>,
    here: Option<Spot>,
    found: RefCell<Seen>,
    /// The transcript's, which the match being shown says where it was drawn in.
    reveal: Reveal,
}

/// Where the needle is in each text drawn since a [`FindLook`] was made, with how long the text was, since a text such
/// as a call's lines can change once its result comes.
type Seen = HashMap<TextId, (usize, Rc<[Range<usize>]>)>;

impl FindLook {
    pub(crate) fn new(needle: Option<Needle>, here: Option<Spot>, reveal: Reveal) -> Self {
        Self { needle, here, found: RefCell::default(), reveal }
    }

    pub(crate) fn reveal(&self) -> &Reveal {
        &self.reveal
    }

    /// Which match in `id` is the one being shown, if it's in that text.
    fn nth_here(&self, id: &TextId) -> Option<usize> {
        self.here.as_ref().filter(|here| here.text == *id).map(|here| here.nth)
    }

    /// Where the needle is in `text`, which is the text `id` as it's drawn, and which of those is the match being
    /// shown.
    pub(crate) fn marks(&self, id: &TextId, text: &str) -> Marks {
        self.marks_of(id, || Cow::Borrowed(text))
    }

    /// How many matches each text drawn since this was made has, for a test.
    #[cfg(feature = "testing")]
    pub(crate) fn drawn(&self) -> Vec<(TextId, usize)> {
        self.found.borrow().iter().map(|(id, (_, ranges))| (id.clone(), ranges.len())).collect()
    }

    /// [`FindLook::marks`] of the text `text` makes, which is only made while something's being found.
    pub(crate) fn marks_of<'t>(&self, id: &TextId, text: impl FnOnce() -> Cow<'t, str>) -> Marks {
        if self.needle.is_none() {
            return Marks::default();
        }
        let text = text();
        self.marks_found(id, text.len(), |needle| needle.ranges(&text).collect())
    }

    /// [`FindLook::marks`] of each of `leaves`, the leaves of the Markdown text `id`, such as its paragraphs, none of
    /// which a match runs on from into the next. They're counted as one text, as find counts them, each a line after
    /// the last. None while nothing's being found.
    pub(crate) fn marks_in_leaves(&self, id: &TextId, leaves: &[String]) -> Vec<Marks> {
        if self.needle.is_none() {
            return Vec::new();
        }
        let starts: Vec<usize> =
            (leaves.iter()).scan(0, |at, leaf| Some(std::mem::replace(at, *at + leaf.len() + 1))).collect();
        let len = starts.last().zip(leaves.last()).map_or(0, |(start, leaf)| start + leaf.len());
        let all = self.marks_found(id, len, |needle| {
            (leaves.iter().zip(&starts))
                .flat_map(|(leaf, start)| needle.ranges(leaf).map(move |range| range.start + start..range.end + start))
                .collect()
        });
        leaves.iter().zip(&starts).map(|(leaf, start)| all.within(*start, start + leaf.len())).collect()
    }

    /// The marks of the text `id`, `len` long, with the needle where `find` says, worked out once for each length
    /// it's drawn at.
    fn marks_found(&self, id: &TextId, len: usize, find: impl FnOnce(&Needle) -> Rc<[Range<usize>]>) -> Marks {
        let Some(needle) = &self.needle else { return Marks::default() };
        let mut found = self.found.borrow_mut();
        let ranges = match found.get(id) {
            Some((was, ranges)) if *was == len => ranges.clone(),
            Some(_) | None => {
                let ranges = find(needle);
                found.insert(id.clone(), (len, ranges.clone()));
                ranges
            }
        };
        let here = self.nth_here(id).filter(|nth| *nth < ranges.len());
        Marks { ranges, here, reveal: here.map(|_| self.reveal.clone()) }
    }
}

/// Where the needle is in one text as it's drawn, in bytes, and which of those is the match being shown.
#[derive(Clone, Debug, Default)]
pub(crate) struct Marks {
    ranges: Rc<[Range<usize>]>,
    here: Option<usize>,
    /// Where the match being shown says it was drawn, if it's here.
    reveal: Option<Reveal>,
}

impl Marks {
    pub(crate) fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// The match being shown, if it's in this text.
    pub(crate) fn here(&self) -> Option<Range<usize>> {
        self.here.and_then(|nth| self.ranges.get(nth)).cloned()
    }

    /// Where the match being shown says it was drawn, if it's in this text.
    pub(crate) fn reveal(&self) -> Option<&Reveal> {
        self.reveal.as_ref().filter(|_| self.here.is_some())
    }

    /// Each match, with the colour behind it.
    pub(crate) fn backgrounds(&self) -> impl Iterator<Item = (Range<usize>, Hsla)> + '_ {
        self.ranges.iter().enumerate().map(|(nth, range)| {
            (range.clone(), if Some(nth) == self.here { look::found_here() } else { look::found() })
        })
    }

    /// These marks for the text drawn after `by` bytes of something else, such as a prompt.
    pub(crate) fn moved(&self, by: usize) -> Marks {
        let ranges = self.ranges.iter().map(|range| range.start + by..range.end + by).collect();
        Marks { ranges, here: self.here, reveal: self.reveal.clone() }
    }

    /// These marks for the part of the text from `start` to `end`, moved to start there, so a line of a text can be
    /// drawn with its own. A match that runs past either end is cut at it.
    pub(crate) fn within(&self, start: usize, end: usize) -> Marks {
        let inside = |range: &Range<usize>| range.start < end && range.end > start;
        let first = self.ranges.iter().position(inside).unwrap_or(0);
        let ranges: Rc<[Range<usize>]> = self
            .ranges
            .iter()
            .filter(|range| inside(range))
            .map(|range| range.start.max(start) - start..range.end.min(end) - start)
            .collect();
        let here = self.here.filter(|nth| self.ranges.get(*nth).is_some_and(inside)).map(|nth| nth - first);
        Marks { ranges, here, reveal: self.reveal.clone() }
    }

    /// `styles`, such as code's colours, with each match's background laid over them.
    pub(crate) fn over(
        &self,
        styles: &[(Range<usize>, HighlightStyle)],
        len: usize,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        if self.is_empty() {
            return styles.to_vec();
        }
        behind(styles, &self.backgrounds().collect::<Vec<_>>(), len)
    }
}

/// `styles`, such as code's colours, over `backgrounds` in a text `len` long. GPUI wants the pieces in order and not
/// overlapping, so they're cut where any of them starts or ends, and where backgrounds overlap the last one shows.
pub(crate) fn behind(
    styles: &[(Range<usize>, HighlightStyle)],
    backgrounds: &[(Range<usize>, Hsla)],
    len: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut cuts = vec![0, len];
    cuts.extend(styles.iter().flat_map(|(range, _)| [range.start, range.end]));
    cuts.extend(backgrounds.iter().flat_map(|(range, _)| [range.start, range.end]));
    cuts.retain(|cut| *cut <= len);
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .filter_map(|pair| {
            let piece = pair[0]..pair[1];
            let inside = |range: &Range<usize>| range.start <= piece.start && piece.end <= range.end;
            let style = styles.iter().find(|(range, _)| inside(range)).map(|(_, style)| *style);
            let background = backgrounds.iter().rev().find(|(range, _)| inside(range)).map(|(_, color)| *color);
            if style.is_none() && background.is_none() {
                return None;
            }
            let mut style = style.unwrap_or_default();
            if background.is_some() {
                style.background_color = background;
            }
            Some((piece, style))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(query: &str, here: Option<usize>) -> (FindLook, TextId) {
        let id = (ItemKey::Event(1, 0), Part::Output);
        let here = here.map(|nth| Spot { text: id.clone(), nth });
        (FindLook::new(Some(Needle::new(query).unwrap()), here, Reveal::default()), id)
    }

    #[test]
    fn a_text_s_marks_say_where_each_match_is_and_which_is_shown() {
        let (look, id) = look("ab", Some(1));

        let marks = look.marks(&id, "ab xab ab");

        assert_eq!(&*marks.ranges, [0..2, 4..6, 7..9]);
        assert_eq!(marks.here(), Some(4..6));
        assert_eq!(look.marks(&(ItemKey::Event(2, 0), Part::Output), "ab").here(), None, "another text's isn't");
    }

    #[test]
    fn a_line_s_marks_are_those_in_it_moved_to_its_start() {
        let (look, id) = look("ab", Some(1));
        let marks = look.marks(&id, "ab\nab ab");

        let second = marks.within(3, 8);

        assert_eq!(&*second.ranges, [0..2, 3..5]);
        assert_eq!(second.here(), Some(0..2), "the match shown is the first on the second line");
        assert_eq!(marks.within(0, 2).here(), None);
    }

    #[test]
    fn matches_lie_over_code_s_colours_cut_where_either_changes() {
        let (look, id) = look("cde", None);
        let marks = look.marks(&id, "abcdef");
        let red = HighlightStyle { color: Some(gpui_kit::red()), ..HighlightStyle::default() };

        let pieces = marks.over(&[(1..4, red)], 6);

        let ranges: Vec<Range<usize>> = pieces.iter().map(|(range, _)| range.clone()).collect();
        assert_eq!(ranges, [1..2, 2..4, 4..5], "nothing past either");
        assert_eq!(pieces[1].1.color, red.color, "the match keeps the code's colour");
        assert!(pieces[1].1.background_color.is_some() && pieces[2].1.background_color.is_some());
        assert_eq!(pieces[2].1.color, None, "and goes on past it");
        assert!(pieces[0].1.background_color.is_none());
    }
}
