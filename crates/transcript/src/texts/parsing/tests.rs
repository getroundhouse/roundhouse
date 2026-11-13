use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::base::text::editor_host::BlockNode;
use gpui_kit::{Task, TestAppContext};

use super::super::ImagesAre::{TheKits, TheTranscripts};
use super::super::{AT_ONCE, Of, Parsed, Text, TextAt, TextKey, Texts, parse};

/// The end of the chat, where a text can go on.
const END: Of = Of::Tail;
/// The row of an event, where a text is once it's recorded.
const ROW: Of = Of::Event(1);

/// What the first paragraph of a text made by [`with_kept`] shows, which a whole parse of it wouldn't, so a parse that
/// kept the blocks before can be told from one that didn't.
const KEPT: &str = "Kept as it was";

fn id() -> TextKey {
    TextKey::of(TextAt::only(1, 0))
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

/// Texts with `parsed` as the text known as [`id`].
fn with(parsed: Parsed) -> Texts {
    let texts = Texts::alone();
    let mut store = texts.0.borrow_mut();
    store.placed(&id(), TextAt::only(1, 0));
    store.texts.insert(id(), Text::Parsed { parsed: Rc::new(parsed) });
    drop(store);
    texts
}

/// Texts with `was` parsed as the text known as [`id`], but with its first leaf saying [`KEPT`].
fn with_kept(was: &str) -> Texts {
    let mut kept = parse::parse(was.to_owned(), TheKits);
    let mut leaves = kept.leaves.to_vec();
    leaves[0] = KEPT.to_owned();
    kept.leaves = Arc::from(leaves);
    with(kept)
}

/// `was`, as [`with_kept`] has it, gone on by a few words at the end of the chat, and what it says then. So it's
/// joined from what it was parsed as and what it went on to.
fn gone_on_at_the_end(was: &str) -> (Texts, String) {
    let texts = with_kept(was);
    let source = format!("{was}And a last few words.");
    texts.parsed(&id(), &source, END, TheKits);
    (texts, source)
}

fn source_of(parsed: Option<Rc<Parsed>>) -> Option<String> {
    parsed.map(|parsed| parsed.source.clone())
}

fn first_leaf(parsed: Option<Rc<Parsed>>) -> Option<String> {
    parsed.map(|parsed| parsed.leaves[0].clone())
}

#[test]
fn a_long_text_that_goes_on_a_little_is_parsed_at_once_from_near_its_end() {
    let was = paragraphs(3 * AT_ONCE);
    let source = format!("{was}And a last few words.");
    let texts = with(parse::parse(was, TheKits));

    let (parsed, apart) = texts.parsed(&id(), &source, END, TheKits);

    let parsed = parsed.expect("drawn at once");
    assert!(apart.is_none(), "with nothing to parse on another thread");
    assert_eq!(parsed.source, source);
    assert_eq!(parsed.leaves, parse::leaves(&source), "as a whole parse would have it");
}

#[test]
fn a_text_that_goes_on_by_more_than_is_parsed_at_once_is_drawn_as_it_was_meanwhile() {
    let was = paragraphs(AT_ONCE / 2);
    let texts = with(parse::parse(was.clone(), TheKits));

    let (parsed, apart) = texts.parsed(&id(), &format!("{was}{}", list(2 * AT_ONCE)), END, TheKits);

    assert_eq!(source_of(parsed), Some(was), "what it was parsed as");
    assert!(apart.is_some(), "while what it went on to is parsed on another thread");
}

/// A text that goes on to define a link is parsed whole, which takes long for a long one, and meanwhile it's what it
/// was, rather than nothing.
#[test]
fn a_long_text_that_goes_on_to_be_parsed_whole_is_drawn_as_it_was_meanwhile() {
    let was = paragraphs(2 * AT_ONCE);
    let texts = with(parse::parse(was.clone(), TheKits));

    let (parsed, apart) = texts.parsed(&id(), &format!("{was}[docs]: https://example.com\n"), END, TheKits);

    assert_eq!(source_of(parsed), Some(was));
    assert!(apart.is_some());
}

#[test]
fn a_long_text_that_changes_other_than_by_going_on_is_drawn_as_it_was_meanwhile_and_parsed_whole() {
    let was = paragraphs(2 * AT_ONCE);
    let texts = with(parse::parse(was.clone(), TheKits));

    let (parsed, apart) = texts.parsed(&id(), &list(2 * AT_ONCE), END, TheKits);

    assert_eq!(source_of(parsed), Some(was), "drawn as it was until it's in");
    assert!(apart.is_some_and(|apart| apart.again.is_none()), "and not parsed again from near its end");
}

#[test]
fn a_text_that_goes_on_from_what_s_being_parsed_apart_waits_for_it() {
    let was = paragraphs(AT_ONCE / 2);
    let parsing = format!("{was}{}", list(2 * AT_ONCE));
    let texts = with(parse::parse(was.clone(), TheKits));
    let old = texts.parsed(&id(), &was, END, TheKits).0;
    let task = Task::ready(());
    texts.0.borrow_mut().texts.insert(id(), Text::Parsing { source: parsing.clone(), was: old, _task: task });

    let (parsed, apart) = texts.parsed(&id(), &format!("{parsing}- One more\n"), END, TheKits);

    assert_eq!(source_of(parsed), Some(was), "drawn as it was");
    assert!(apart.is_none(), "and no other parse started");
}

/// The blocks before where it's parsed again from are kept, as a leaf of them that says something a whole parse
/// wouldn't shows.
#[gpui_kit::test]
fn a_text_parsed_apart_from_near_its_end_keeps_what_was_before(cx: &mut TestAppContext) {
    let was = paragraphs(AT_ONCE / 2);
    let source = format!("{was}{}", list(2 * AT_ONCE));
    let texts = with_kept(&was);
    let (_, apart) = texts.parsed(&id(), &source, END, TheKits);

    cx.update(|cx| texts.parse_apart(id(), apart.expect("parsed apart"), cx));
    cx.run_until_parked();

    let (parsed, _) = texts.parsed(&id(), &source, END, TheKits);
    let parsed = parsed.expect("parsed");
    assert_eq!(parsed.leaves[0], KEPT);
    assert_eq!(parsed.leaves[1..], parse::leaves(&source)[1..], "and the rest as a whole parse would have it");
}

/// A text that goes on while it's being parsed apart waits, and once that parse is in, goes on from it.
#[gpui_kit::test]
fn a_text_that_went_on_while_parsed_apart_goes_on_from_that_parse(cx: &mut TestAppContext) {
    let was = paragraphs(AT_ONCE / 2);
    let parsing = format!("{was}{}", list(2 * AT_ONCE));
    let texts = with(parse::parse(was.clone(), TheKits));
    let (_, apart) = texts.parsed(&id(), &parsing, END, TheKits);
    cx.update(|cx| texts.parse_apart(id(), apart.expect("parsed apart"), cx));
    let more = format!("{parsing}- One more\n");

    let waiting = source_of(texts.parsed(&id(), &more, END, TheKits).0);
    cx.run_until_parked();
    let (parsed, apart) = texts.parsed(&id(), &more, END, TheKits);

    assert_eq!(waiting, Some(was), "drawn as it was while the parse is apart");
    assert_eq!(source_of(parsed), Some(parsing), "and as that parse once it's in");
    assert!(apart.is_some(), "while what it's gone on to since is parsed");
}

/// What a text was parsed as from near its end, as it was written, can differ from a whole parse, so it's parsed
/// whole the first time it's drawn in the row of its record, and that's kept.
#[test]
fn a_text_joined_as_it_went_on_is_parsed_whole_once_away_from_the_end_of_the_chat() {
    let (texts, source) = gone_on_at_the_end(&paragraphs(AT_ONCE / 2));

    let (parsed, apart) = texts.parsed(&id(), &source, ROW, TheKits);
    let (again, _) = texts.parsed(&id(), &source, ROW, TheKits);

    let (parsed, again) = (parsed.expect("drawn at once"), again.expect("drawn"));
    assert!(apart.is_none(), "a short one where it's drawn");
    assert_eq!(parsed.leaves, parse::leaves(&source), "as a whole parse has it");
    assert!(Rc::ptr_eq(&parsed, &again), "and not parsed again after");
}

#[test]
fn a_text_still_at_the_end_of_the_chat_is_left_as_it_was_joined() {
    let (texts, source) = gone_on_at_the_end(&paragraphs(AT_ONCE / 2));

    let (parsed, apart) = texts.parsed(&id(), &source, END, TheKits);

    assert!(apart.is_none());
    assert_eq!(first_leaf(parsed).as_deref(), Some(KEPT), "since it can go on from there");
}

/// A reply of one paragraph is parsed again from its start each time it goes on, which is parsing it whole, so it
/// isn't parsed once more in the row of its record.
#[test]
fn a_text_parsed_again_from_its_start_as_it_went_on_is_whole_already() {
    let was = "One paragraph of a reply";
    let source = format!("{was}, and a few more words of it.");
    let texts = with(parse::parse(was.to_owned(), TheKits));
    let (at_the_end, _) = texts.parsed(&id(), &source, END, TheKits);

    let (in_its_row, apart) = texts.parsed(&id(), &source, ROW, TheKits);

    assert!(apart.is_none());
    assert!(Rc::ptr_eq(&at_the_end.expect("drawn"), &in_its_row.expect("drawn")), "the same parse, not another");
}

/// A reply is parsed for the kit to draw its images while it's written, when the hub has kept none, and for the
/// transcript to draw them in the row of its record, so the parse it was written into isn't its record's.
#[test]
fn a_text_parsed_for_the_kit_to_draw_its_images_is_parsed_again_for_the_transcript_to() {
    let source = "The board, after:\n\n![The board](board.png)";
    let texts = with(parse::parse(source.to_owned(), TheKits));
    let (at_the_end, _) = texts.parsed(&id(), source, END, TheKits);

    let (in_its_row, apart) = texts.parsed(&id(), source, ROW, TheTranscripts);
    let (again, _) = texts.parsed(&id(), source, ROW, TheTranscripts);

    let (at_the_end, in_its_row) = (at_the_end.expect("drawn"), in_its_row.expect("drawn at once"));
    let its_image = |parsed: &Parsed| matches!(parsed.blocks.last(), Some(BlockNode::Custom { .. }));
    assert!(apart.is_none(), "a short one where it's drawn");
    assert!(!its_image(&at_the_end) && its_image(&in_its_row), "its image is the transcript's only in its row");
    assert!(Rc::ptr_eq(&in_its_row, &again.expect("drawn")), "and it's not parsed again after");
}

#[gpui_kit::test]
fn a_long_text_joined_as_it_went_on_is_drawn_as_it_was_while_it_s_parsed_whole(cx: &mut TestAppContext) {
    let (texts, source) = gone_on_at_the_end(&paragraphs(3 * AT_ONCE));

    let (meanwhile, apart) = texts.parsed(&id(), &source, ROW, TheKits);
    cx.update(|cx| texts.parse_apart(id(), apart.expect("parsed on another thread"), cx));
    cx.run_until_parked();
    let (parsed, apart) = texts.parsed(&id(), &source, ROW, TheKits);

    assert_eq!(first_leaf(meanwhile).as_deref(), Some(KEPT), "drawn as it was meanwhile");
    assert!(apart.is_none(), "once");
    assert_eq!(parsed.expect("parsed").leaves, parse::leaves(&source), "as a whole parse has it");
}

/// A long reply written fast is recorded while what it went on to is still being parsed on another thread. Its row
/// waits for that parse, which is joined, and then has it parsed whole.
#[gpui_kit::test]
fn a_text_recorded_while_it_s_parsed_apart_from_near_its_end_is_parsed_whole_after(cx: &mut TestAppContext) {
    let was = paragraphs(AT_ONCE / 2);
    let source = format!("{was}{}", list(2 * AT_ONCE));
    let texts = with_kept(&was);
    let (_, apart) = texts.parsed(&id(), &source, END, TheKits);
    cx.update(|cx| texts.parse_apart(id(), apart.expect("parsed apart"), cx));

    let (waiting, started) = texts.parsed(&id(), &source, ROW, TheKits);
    cx.run_until_parked();
    let (joined, apart) = texts.parsed(&id(), &source, ROW, TheKits);
    cx.update(|cx| texts.parse_apart(id(), apart.expect("parsed whole on another thread"), cx));
    cx.run_until_parked();
    let (parsed, apart) = texts.parsed(&id(), &source, ROW, TheKits);

    assert_eq!(source_of(waiting), Some(was), "drawn as it was while the parse is apart");
    assert!(started.is_none(), "with no other parse started");
    assert_eq!(first_leaf(joined).as_deref(), Some(KEPT), "then as that parse, which kept what was before");
    assert!(apart.is_none(), "once");
    assert_eq!(parsed.expect("parsed").leaves, parse::leaves(&source), "as a whole parse has it");
}

/// A record that says more than was written of it isn't parsed from near its end either, since it won't go on.
#[test]
fn a_text_that_says_more_away_from_the_end_of_the_chat_is_parsed_whole() {
    let (texts, source) = gone_on_at_the_end(&paragraphs(AT_ONCE / 2));
    let more = format!("{source} And then some.");

    let (parsed, _) = texts.parsed(&id(), &more, ROW, TheKits);

    assert_eq!(parsed.expect("drawn at once").leaves, parse::leaves(&more));
}
