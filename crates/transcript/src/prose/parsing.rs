//! When and where a text is parsed: where it's drawn, when what's to be parsed of it is short, and otherwise on another
//! thread, as [`Prose::parsed`] says.

use std::rc::Rc;

use gpui_kit::App;

use super::parse::{self, Again};
use super::{AT_ONCE, Parsed, Prose, Text, TextAt};

/// How a text is to be parsed on another thread: `source` whole, or again from where `again` says, as it goes on from
/// `was`, which it's drawn as until it's in, if it goes on from that.
pub(super) struct Apart {
    source: String,
    was: Option<Rc<Parsed>>,
    again: Option<Again>,
}

impl Prose {
    /// What the text at `at` is drawn as now that it says `source`, if it has a parse to draw, and how it's to be
    /// parsed on another thread, if it is. A text is parsed where it's drawn when what's to be parsed of it is short:
    /// all of it, or for one that goes on from what it was parsed as, what's parsed again from near its end. A longer
    /// one is parsed on another thread, and one that goes on is drawn as it was meanwhile. While a parse is on another
    /// thread, another isn't started for a text that goes on from what that's parsing, so a text that goes on faster
    /// than a parse ends, such as one the agent is writing, is drawn as each parse comes in.
    pub(super) fn parsed(&self, at: TextAt, source: &str) -> (Option<Rc<Parsed>>, Option<Apart>) {
        let mut texts = self.0.borrow_mut();
        let now = texts.frame;
        let was = match texts.texts.get_mut(&at) {
            Some(Text::Parsed { parsed, drawn }) if parsed.source == source => {
                *drawn = now;
                return (Some(parsed.clone()), None);
            }
            Some(Text::Parsing { source: parsing, was, .. }) if source.starts_with(parsing.as_str()) => {
                return (was.clone(), None);
            }
            Some(text) => text.was().filter(|was| source.starts_with(was.source.as_str())),
            None => None,
        };
        let again = was.as_deref().and_then(|was| parse::again(was, source));
        if again.map_or(source.len(), |again| source.len() - again.byte) > AT_ONCE {
            return (was.clone(), Some(Apart { source: source.to_owned(), was, again }));
        }
        let parsed = Rc::new(match (was, again) {
            (Some(was), Some(again)) => parse::gone_on(&was, again, source.to_owned()),
            _ => parse::parse(source.to_owned()),
        });
        texts.texts.insert(at, Text::Parsed { parsed: parsed.clone(), drawn: now });
        texts.parsed_one();
        (Some(parsed), None)
    }

    /// Parses the text at `at` on another thread, as `apart` says, unless what it says goes on from what's being
    /// parsed there, and measures its row again once it's in. It says what it said, so there's nothing new to find in
    /// it.
    pub(super) fn parse_apart(&self, at: TextAt, apart: Apart, cx: &mut App) {
        let Apart { source, was, again } = apart;
        let mut texts = self.0.borrow_mut();
        if texts.texts.get(&at).is_some_and(|text| text.parsing().is_some_and(|parsing| source.starts_with(parsing))) {
            return;
        }
        let (prose, transcript, given) = (Rc::downgrade(&self.0), texts.transcript.clone(), source.clone());
        let from = was.clone().zip(again);
        let parsing = cx.spawn(async move |cx| {
            let background = cx.background_executor().clone();
            let rest = match &from {
                Some((_, again)) => {
                    let rest = given[again.byte..].to_owned();
                    background.spawn(async move { parse::read(&rest) }).await
                }
                None => None,
            };
            let parsed = match (from, rest) {
                (Some((was, again)), Some(rest)) => parse::joined(&was, again, given, rest),
                _ => background.spawn(async move { parse::parse(given) }).await,
            };
            let Some(prose) = prose.upgrade() else { return };
            // What it replaces holds this task, which goes once it's done.
            let _was = {
                let mut texts = prose.borrow_mut();
                if texts.texts.get(&at).and_then(Text::parsing) != Some(parsed.source.as_str()) {
                    return;
                }
                let drawn = texts.frame;
                texts.texts.insert(at, Text::Parsed { parsed: Rc::new(parsed), drawn })
            };
            let _ = transcript.update(cx, |transcript, cx| transcript.redrawn(at.of, cx));
        });
        texts.texts.insert(at, Text::Parsing { source, was, _task: parsing });
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui_kit::{Task, TestAppContext, WeakEntity};

    use super::super::{AT_ONCE, Parsed, Prose, Text, TextAt, parse};

    fn at() -> TextAt {
        TextAt::only(1, 0)
    }

    /// Paragraphs of a reply, `bytes` long or a little more.
    fn paragraphs(bytes: usize) -> String {
        let paragraph = "The signal box keeps a ledger of every train that passes, and the `ledger` is **kept**.\n\n";
        paragraph.repeat(bytes / paragraph.len() + 1)
    }

    /// A list, one block, `bytes` long or a little more.
    fn list(bytes: usize) -> String {
        let item = "- An item of the list, with `code` and **emphasis** on it\n";
        item.repeat(bytes / item.len() + 1)
    }

    /// `prose` with `parsed` as the text at [`at`].
    fn with(parsed: Parsed) -> Prose {
        let prose = Prose::new(WeakEntity::new_invalid());
        prose.0.borrow_mut().texts.insert(at(), Text::Parsed { parsed: Rc::new(parsed), drawn: 0 });
        prose
    }

    fn source_of(parsed: Option<Rc<Parsed>>) -> Option<String> {
        parsed.map(|parsed| parsed.source.clone())
    }

    #[test]
    fn a_long_text_that_goes_on_a_little_is_parsed_at_once_from_near_its_end() {
        let was = paragraphs(3 * AT_ONCE);
        let source = format!("{was}And a last few words.");
        let prose = with(parse::parse(was));

        let (parsed, apart) = prose.parsed(at(), &source);

        let parsed = parsed.expect("drawn at once");
        assert!(apart.is_none(), "with nothing to parse on another thread");
        assert_eq!(parsed.source, source);
        assert_eq!(parsed.leaves, parse::parse(source.clone()).leaves, "as a whole parse would have it");
    }

    #[test]
    fn a_text_that_goes_on_by_more_than_is_parsed_at_once_is_drawn_as_it_was_meanwhile() {
        let was = paragraphs(AT_ONCE / 2);
        let prose = with(parse::parse(was.clone()));

        let (parsed, apart) = prose.parsed(at(), &format!("{was}{}", list(2 * AT_ONCE)));

        assert_eq!(source_of(parsed), Some(was), "what it was parsed as");
        assert!(apart.is_some(), "while what it went on to is parsed on another thread");
    }

    /// A text that goes on to define a link is parsed whole, which takes long for a long one, and meanwhile it's what
    /// it was, rather than nothing.
    #[test]
    fn a_long_text_that_goes_on_to_be_parsed_whole_is_drawn_as_it_was_meanwhile() {
        let was = paragraphs(2 * AT_ONCE);
        let prose = with(parse::parse(was.clone()));

        let (parsed, apart) = prose.parsed(at(), &format!("{was}[docs]: https://example.com\n"));

        assert_eq!(source_of(parsed), Some(was));
        assert!(apart.is_some());
    }

    #[test]
    fn a_long_text_that_changes_other_than_by_going_on_isn_t_drawn_as_it_was() {
        let prose = with(parse::parse(paragraphs(2 * AT_ONCE)));

        let (parsed, apart) = prose.parsed(at(), &list(2 * AT_ONCE));

        assert_eq!(source_of(parsed), None, "drawn as nothing until it's in");
        assert!(apart.is_some());
    }

    #[test]
    fn a_text_that_goes_on_from_what_s_being_parsed_apart_waits_for_it() {
        let was = paragraphs(AT_ONCE / 2);
        let parsing = format!("{was}{}", list(2 * AT_ONCE));
        let prose = with(parse::parse(was.clone()));
        let old = prose.parsed(at(), &was).0;
        let task = Task::ready(());
        prose.0.borrow_mut().texts.insert(at(), Text::Parsing { source: parsing.clone(), was: old, _task: task });

        let (parsed, apart) = prose.parsed(at(), &format!("{parsing}- One more\n"));

        assert_eq!(source_of(parsed), Some(was), "drawn as it was");
        assert!(apart.is_none(), "and no other parse started");
    }

    /// The blocks before where it's parsed again from are kept, as a leaf of them that says something a whole parse
    /// wouldn't shows.
    #[gpui_kit::test]
    fn a_text_parsed_apart_from_near_its_end_keeps_what_was_before(cx: &mut TestAppContext) {
        let was = paragraphs(AT_ONCE / 2);
        let source = format!("{was}{}", list(2 * AT_ONCE));
        let mut kept = parse::parse(was);
        let mut leaves = kept.leaves.to_vec();
        leaves[0] = "Kept as it was".to_owned();
        kept.leaves = Arc::from(leaves);
        let prose = with(kept);
        let (_, apart) = prose.parsed(at(), &source);

        cx.update(|cx| prose.parse_apart(at(), apart.expect("parsed apart"), cx));
        cx.run_until_parked();

        let (parsed, _) = prose.parsed(at(), &source);
        let parsed = parsed.expect("parsed");
        let whole = parse::parse(source.clone()).leaves;
        assert_eq!(parsed.leaves[0], "Kept as it was");
        assert_eq!(parsed.leaves[1..], whole[1..], "and the rest as a whole parse would have it");
    }

    /// A text that goes on while it's being parsed apart waits, and once that parse is in, goes on from it.
    #[gpui_kit::test]
    fn a_text_that_went_on_while_parsed_apart_goes_on_from_that_parse(cx: &mut TestAppContext) {
        let was = paragraphs(AT_ONCE / 2);
        let parsing = format!("{was}{}", list(2 * AT_ONCE));
        let prose = with(parse::parse(was.clone()));
        let (_, apart) = prose.parsed(at(), &parsing);
        cx.update(|cx| prose.parse_apart(at(), apart.expect("parsed apart"), cx));
        let more = format!("{parsing}- One more\n");

        let waiting = source_of(prose.parsed(at(), &more).0);
        cx.run_until_parked();
        let (parsed, apart) = prose.parsed(at(), &more);

        assert_eq!(waiting, Some(was), "drawn as it was while the parse is apart");
        assert_eq!(source_of(parsed), Some(parsing), "and as that parse once it's in");
        assert!(apart.is_some(), "while what it's gone on to since is parsed");
    }
}
