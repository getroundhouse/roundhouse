use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::TestAppContext;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::Seq;

use super::super::{AT_ONCE, ImagesAre, Of, Text, TextAt, TextKey, Texts, Written};
use crate::fixtures::{event, log};
use crate::selection::Selection;

fn at(seq: Seq) -> TextAt {
    TextAt::only(seq, 0)
}

/// What's parsed of the text known as `id`, if it's parsed.
fn parse_of(texts: &Texts, id: &TextKey) -> Option<Rc<super::super::Parsed>> {
    match texts.0.borrow().texts.get(id)? {
        Text::Parsed { parsed } => Some(parsed.clone()),
        Text::Read { .. } | Text::Parsing { .. } | Text::Plain { .. } => None,
    }
}

/// Draws the text known as `id`, saying `text`, at `at`, in a frame of its own, as what draws that place would.
fn drawn(texts: &Texts, selection: &Selection, id: &TextKey, at: TextAt, text: &str) {
    texts.new_frame();
    texts.drawing(at.of, || texts.draw(selection, &Written::unfound(at, id.0.clone(), text), None));
}

/// A log with a reply at each of `seqs`, so their rows are loaded.
fn replies_at(seqs: impl IntoIterator<Item = Seq>) -> client_core::session_log::SessionLog {
    log(seqs.into_iter().map(|seq| {
        let blocks = vec![ContentBlock::Text { text: "words".into(), images: Vec::new() }];
        event(
            seq,
            EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None },
        )
    }))
}

#[test]
fn a_text_drawn_somewhere_new_is_there_with_what_it_was_parsed_as() {
    let texts = Texts::alone();
    let reply = TextKey::named("reply");
    texts.drawn_as(&reply, at(Seq::new(1)), "What it said");
    let parsed = parse_of(&texts, &reply);
    texts.new_frame();

    texts.0.borrow_mut().placed(&reply, at(Seq::new(2)));

    assert_eq!(texts.place_of(&reply), Some(at(Seq::new(2))));
    assert_eq!((texts.known_at(at(Seq::new(2))), texts.known_at(at(Seq::new(1)))), (Some(reply.clone()), None));
    assert!(parse_of(&texts, &reply).zip(parsed).is_some_and(|(now, was)| Rc::ptr_eq(&now, &was)), "not parsed again");
    assert_eq!(
        texts.leaves_of(&reply).map(|leaves| leaves.each().collect::<Vec<_>>().join("\n")).as_deref(),
        Some("What it said")
    );
}

/// The list draws the end of the chat before the rows above it, so a prompt waiting there can take the place of one
/// that's moved to the row where the agent took it, before that row is drawn.
#[test]
fn a_text_drawn_where_another_was_takes_its_place_and_the_other_keeps_its_parse() {
    let texts = Texts::alone();
    let (first, second) = (TextKey::named("first"), TextKey::named("second"));
    texts.drawn_as(&first, at(Seq::new(9)), "First");
    let parsed = parse_of(&texts, &first);
    texts.new_frame();

    texts.drawn_as(&second, at(Seq::new(9)), "Second");
    let place = texts.place_of(&first);
    texts.0.borrow_mut().placed(&first, at(Seq::new(3)));

    assert_eq!(texts.known_at(at(Seq::new(9))), Some(second));
    assert_eq!(place, None, "the first has no place until it's drawn again");
    assert!(parse_of(&texts, &first).zip(parsed).is_some_and(|(now, was)| Rc::ptr_eq(&now, &was)), "not parsed again");
}

/// The texts are let go of each time the log changes, which can be before the row a text has moved to is drawn.
#[test]
fn a_text_whose_place_another_has_taken_is_kept_as_long_as_it_would_have_been_there() {
    let texts = Texts::alone();
    let (first, second) = (TextKey::named("first"), TextKey::named("second"));
    texts.drawn_as(&first, at(Seq::new(9)), "First");
    texts.new_frame();
    texts.drawn_as(&second, at(Seq::new(9)), "Second");

    texts.let_go(&replies_at([Seq::new(9)]), None);
    let kept = (texts.keeps(&first), texts.last_drawn_at(&first));
    texts.let_go(&replies_at([]), None);

    assert_eq!(kept, (true, Some(at(Seq::new(9)))), "while where it was last drawn is loaded");
    assert!(!texts.keeps(&first) && !texts.keeps(&second), "and no longer");
}

/// A long text's parse on another thread lands where it is once it's in, as a reply's does when it's recorded
/// meanwhile, and the row that draws it there is measured again.
#[gpui_kit::test]
fn a_text_parsed_apart_is_parsed_wherever_it_s_moved_to_meanwhile(cx: &mut TestAppContext) {
    let redrawn = Rc::new(RefCell::new(Vec::new()));
    let texts = Texts::new({
        let redrawn = redrawn.clone();
        move |of, _| redrawn.borrow_mut().push(of)
    });
    let reply = TextKey::named("reply");
    let long = "The signal box keeps a ledger.\n\n".repeat(2 * AT_ONCE / 32);
    texts.0.borrow_mut().placed(&reply, at(Seq::new(1)));
    let (_, apart) = texts.parsed(&reply, &long, at(Seq::new(1)).of, ImagesAre::TheKits);
    cx.update(|cx| texts.parse_apart(reply.clone(), apart.expect("parsed apart"), cx));
    texts.new_frame();

    texts.0.borrow_mut().placed(&reply, at(Seq::new(2)));
    cx.run_until_parked();

    assert!(parse_of(&texts, &reply).is_some_and(|parsed| parsed.source == long));
    assert_eq!(*redrawn.borrow(), [Of::Event(Seq::new(2))]);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "drawn at TextAt")]
fn a_text_drawn_in_two_places_in_one_frame_is_caught() {
    let texts = Texts::alone();
    let reply = TextKey::named("reply");
    texts.drawn_as(&reply, at(Seq::new(1)), "What it said");

    texts.drawn_as(&reply, at(Seq::new(2)), "What it said");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "both drawn at")]
fn two_texts_drawn_in_one_place_in_one_frame_are_caught() {
    let texts = Texts::alone();
    texts.drawn_as(&TextKey::named("first"), at(Seq::new(1)), "First");

    texts.drawn_as(&TextKey::named("second"), at(Seq::new(1)), "Second");
}

#[test]
fn a_text_that_s_let_go_of_has_no_place_and_is_not_kept() {
    let texts = Texts::alone();
    let reply = TextKey::of(at(Seq::new(1)));
    texts.drawn_at(at(Seq::new(1)), "What it said");

    texts.let_go(&log([]), None);

    assert_eq!((texts.place_of(&reply), texts.known_at(at(Seq::new(1)))), (None, None));
    assert!(!texts.keeps(&reply));
}

/// Replies the agent was stopped from writing before they were recorded, each taking the place at the end of the chat
/// of the one before, as the next it writes does.
#[test]
fn the_end_of_the_chat_keeps_only_the_few_texts_it_drew_last_and_doesn_t_draw_now() {
    let texts = Texts::alone();
    let (selected, held) = (TextKey::named("selected"), TextAt::at_the_end(5));
    texts.drawn_as(&selected, held, "Selected");
    texts.new_frame();
    texts.drawn_as(&TextKey::named("after it"), held, "After it");
    let stopped: Vec<TextKey> = (0..12).map(|n| TextKey::named(format!("stopped {n}"))).collect();
    for reply in &stopped {
        texts.new_frame();
        texts.drawn_as(reply, TextAt::at_the_end(0), "Half a reply");
    }
    texts.new_frame();
    texts.drawn_as(&stopped[0], TextAt::at_the_end(1), "Half a reply");

    texts.let_go(&log([]), Some(&(held..=held)));

    let kept: Vec<bool> = stopped.iter().map(|reply| texts.keeps(reply)).collect();
    // The first and the last have a place there, and of the others, the last eight were drawn there most recently.
    let expected: Vec<bool> = (0..12).map(|n| n == 0 || n >= 3).collect();
    assert_eq!(kept, expected);
    assert!(texts.keeps(&selected), "and one that's selected");
    texts.let_go(&log([]), None);
    assert!(!texts.keeps(&selected), "until it isn't");
}

/// A reply the agent was stopped from writing, with nothing written after it, is hidden once the end of the chat no
/// longer draws it, and is one of those left there, which the end keeps only a few of.
#[gpui_kit::test]
fn a_reply_the_end_of_the_chat_no_longer_draws_is_left_there(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let selection = Selection::new(texts.clone(), cx.update(|cx| cx.focus_handle()));
    let stopped: Vec<TextKey> = (0..9).map(|n| TextKey::named(format!("stopped {n}"))).collect();
    for reply in &stopped {
        drawn(&texts, &selection, reply, TextAt::at_the_end(0), "Half a reply");
    }
    texts.new_frame();
    texts.drawing(Of::Tail, || ());

    texts.let_go(&log([]), None);

    assert!(!texts.keeps(&stopped[0]), "the one left there longest ago goes");
    assert!(
        stopped[1..].iter().all(|reply| texts.keeps(reply)),
        "and the last, which is hidden, is kept with the others"
    );
}

/// A reply is parsed from near its end as it's written, at the end of the chat, and whole once it's drawn in the row
/// of its record, since `draw` goes by where a text is drawn. `parsing` has the rest.
#[gpui_kit::test]
fn a_reply_joined_as_it_was_written_is_parsed_whole_in_the_row_of_its_record(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let selection = Selection::new(texts.clone(), cx.update(|cx| cx.focus_handle()));
    let reply = TextKey::named("reply");
    let was = "The first paragraph of a reply.\n\nIts second";
    let said = format!("{was}, which goes on.");
    drawn(&texts, &selection, &reply, TextAt::at_the_end(0), was);

    drawn(&texts, &selection, &reply, TextAt::at_the_end(0), &said);
    let written = parse_of(&texts, &reply).map(|parsed| parsed.whole);
    drawn(&texts, &selection, &reply, at(Seq::new(1)), &said);
    let recorded = parse_of(&texts, &reply).map(|parsed| parsed.whole);

    assert_eq!(written, Some(false), "joined from near its end as it's written");
    assert_eq!(recorded, Some(true), "and parsed whole in the row of its record");
}

/// Something done to the texts: one drawn somewhere, maybe in a new frame, or all let go of but those whose rows are
/// loaded.
#[derive(Clone, Debug)]
enum Done {
    Drawn { text: u8, seq: Seq, next_frame: bool },
    LetGo { loaded: Vec<Seq> },
}

fn done() -> impl Strategy<Value = Done> {
    prop_oneof![
        4 => (0..4u8, (1..5u64).prop_map(Seq::new), any::<bool>()).prop_map(|(text, seq, next_frame)| Done::Drawn { text, seq, next_frame }),
        1 => vec((1..5u64).prop_map(Seq::new), 0..4).prop_map(|mut loaded| {
            loaded.sort_unstable();
            loaded.dedup();
            Done::LetGo { loaded }
        }),
    ]
}

proptest! {
    #![proptest_config(Config { rng_seed: RngSeed::Fixed(1), failure_persistence: None, ..Config::default() })]

    /// However texts are drawn and let go of, each with a place is the one at that place, each place has the text
    /// last drawn there, and what's kept of each is kept while it has somewhere it was last drawn.
    #[test]
    fn where_each_text_is_and_which_is_at_each_place_agree(steps in vec(done(), 1..40)) {
        let texts = Texts::alone();
        // Which text was drawn at each place last, and where each text was drawn last, as nothing's been let go of.
        let mut last_at = std::collections::HashMap::new();
        for step in steps {
            match step {
                Done::Drawn { text, seq, next_frame } => {
                    if next_frame {
                        texts.new_frame();
                    }
                    let id = TextKey::named(format!("{text}"));
                    let frame = texts.0.borrow().frame;
                    let store = texts.0.borrow();
                    // A text is drawn in one place a frame, and a place has one text a frame.
                    let elsewhere = store.places.last_at(&id).is_some_and(|was| was != at(seq))
                        && store.places.last_drawn(&id) == Some(frame);
                    let taken = store.places.at(&at(seq)).is_some_and(|other| {
                        *other != id && store.places.last_drawn(other) == Some(frame)
                    });
                    drop(store);
                    if !elsewhere && !taken {
                        texts.drawn_as(&id, at(seq), "words");
                        last_at.insert(id, at(seq));
                    }
                }
                Done::LetGo { loaded } => {
                    texts.let_go(&replies_at(loaded.iter().copied()), None);
                    last_at.retain(|_, place: &mut TextAt| loaded.contains(&place.seq().unwrap_or(Seq::new(0))));
                }
            }
        }

        let store = texts.0.borrow();
        for (id, place) in &last_at {
            prop_assert_eq!(store.places.last_at(id), Some(*place));
            prop_assert!(store.texts.contains_key(id), "{:?} is kept", id);
            let now = store.places.of(id);
            prop_assert!(now.is_none() || now == Some(*place));
            prop_assert_eq!(now.is_some(), store.places.at(place) == Some(id));
        }
        for (place, id) in store.places.within(..) {
            prop_assert_eq!(store.places.of(id), Some(*place));
        }
        prop_assert_eq!(store.texts.len(), last_at.len(), "and nothing else is");
        prop_assert!(store.hidden.iter().all(|id| last_at.contains_key(id)));
    }
}
