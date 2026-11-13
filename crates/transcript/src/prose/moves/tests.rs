use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::TestAppContext;
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngSeed};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::Seq;

use super::super::{AT_ONCE, Of, Prose, ProseId, Text, TextAt};
use crate::fixtures::{event, log};

fn at(seq: Seq) -> TextAt {
    TextAt::only(seq, 0)
}

/// What's parsed of the text known as `id`, if it's parsed.
fn parse_of(prose: &Prose, id: &ProseId) -> Option<Rc<super::super::Parsed>> {
    match prose.0.borrow().texts.get(id)? {
        Text::Parsed { parsed } => Some(parsed.clone()),
        Text::Read { .. } | Text::Parsing { .. } => None,
    }
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
    let prose = Prose::alone();
    let reply = ProseId::named("reply");
    prose.drawn_as(&reply, at(1), "What it said");
    let parsed = parse_of(&prose, &reply);
    prose.new_frame();

    prose.0.borrow_mut().placed(&reply, at(2));

    assert_eq!(prose.place_of(&reply), Some(at(2)));
    assert_eq!((prose.known_at(at(2)), prose.known_at(at(1))), (Some(reply.clone()), None));
    assert!(parse_of(&prose, &reply).zip(parsed).is_some_and(|(now, was)| Rc::ptr_eq(&now, &was)), "not parsed again");
    assert_eq!(prose.leaves_of(&reply).as_deref(), Some(&["What it said".to_owned()][..]));
}

/// The list draws the end of the chat before the rows above it, so a prompt waiting there can take the place of one
/// that's moved to the row where the agent took it, before that row is drawn.
#[test]
fn a_text_drawn_where_another_was_takes_its_place_and_the_other_keeps_its_parse() {
    let prose = Prose::alone();
    let (first, second) = (ProseId::named("first"), ProseId::named("second"));
    prose.drawn_as(&first, at(9), "First");
    let parsed = parse_of(&prose, &first);
    prose.new_frame();

    prose.drawn_as(&second, at(9), "Second");
    let place = prose.place_of(&first);
    prose.0.borrow_mut().placed(&first, at(3));

    assert_eq!(prose.known_at(at(9)), Some(second));
    assert_eq!(place, None, "the first has no place until it's drawn again");
    assert!(parse_of(&prose, &first).zip(parsed).is_some_and(|(now, was)| Rc::ptr_eq(&now, &was)), "not parsed again");
}

/// The texts are let go of each time the log changes, which can be before the row a text has moved to is drawn.
#[test]
fn a_text_whose_place_another_has_taken_is_kept_as_long_as_it_would_have_been_there() {
    let prose = Prose::alone();
    let (first, second) = (ProseId::named("first"), ProseId::named("second"));
    prose.drawn_as(&first, at(9), "First");
    prose.new_frame();
    prose.drawn_as(&second, at(9), "Second");

    prose.let_go(&replies_at([9]), None);
    let kept = (prose.keeps(&first), prose.last_drawn_at(&first));
    prose.let_go(&replies_at([]), None);

    assert_eq!(kept, (true, Some(at(9))), "while where it was last drawn is loaded");
    assert!(!prose.keeps(&first) && !prose.keeps(&second), "and no longer");
}

/// A long text's parse on another thread lands where it is once it's in, as a reply's does when it's recorded
/// meanwhile, and the row that draws it there is measured again.
#[gpui_kit::test]
fn a_text_parsed_apart_is_parsed_wherever_it_s_moved_to_meanwhile(cx: &mut TestAppContext) {
    let redrawn = Rc::new(RefCell::new(Vec::new()));
    let prose = Prose::new({
        let redrawn = redrawn.clone();
        move |of, _| redrawn.borrow_mut().push(of)
    });
    let reply = ProseId::named("reply");
    let long = "The signal box keeps a ledger.\n\n".repeat(2 * AT_ONCE / 32);
    prose.0.borrow_mut().placed(&reply, at(1));
    let (_, apart) = prose.parsed(&reply, &long);
    cx.update(|cx| prose.parse_apart(reply.clone(), apart.expect("parsed apart"), cx));
    prose.new_frame();

    prose.0.borrow_mut().placed(&reply, at(2));
    cx.run_until_parked();

    assert!(parse_of(&prose, &reply).is_some_and(|parsed| parsed.source == long));
    assert_eq!(*redrawn.borrow(), [Of::Event(2)]);
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "drawn at TextAt")]
fn a_text_drawn_in_two_places_in_one_frame_is_caught() {
    let prose = Prose::alone();
    let reply = ProseId::named("reply");
    prose.drawn_as(&reply, at(1), "What it said");

    prose.drawn_as(&reply, at(2), "What it said");
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "both drawn at")]
fn two_texts_drawn_in_one_place_in_one_frame_are_caught() {
    let prose = Prose::alone();
    prose.drawn_as(&ProseId::named("first"), at(1), "First");

    prose.drawn_as(&ProseId::named("second"), at(1), "Second");
}

#[test]
fn a_text_that_s_let_go_of_has_no_place_and_is_not_kept() {
    let prose = Prose::alone();
    let reply = ProseId::of(at(1));
    prose.drawn_at(at(1), "What it said");

    prose.let_go(&log([]), None);

    assert_eq!((prose.place_of(&reply), prose.known_at(at(1))), (None, None));
    assert!(!prose.keeps(&reply));
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
        4 => (0..4u8, 1..5u64, any::<bool>()).prop_map(|(text, seq, next_frame)| Done::Drawn { text, seq, next_frame }),
        1 => vec(1..5u64, 0..4).prop_map(|mut loaded| {
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
        let prose = Prose::alone();
        // Which text was drawn at each place last, and where each text was drawn last, as nothing's been let go of.
        let mut last_at = std::collections::HashMap::new();
        for step in steps {
            match step {
                Done::Drawn { text, seq, next_frame } => {
                    if next_frame {
                        prose.new_frame();
                    }
                    let id = ProseId::named(format!("{text}"));
                    let frame = prose.0.borrow().frame;
                    let store = prose.0.borrow();
                    // A text is drawn in one place a frame, and a place has one text a frame.
                    let elsewhere = store.places.last_at(&id).is_some_and(|was| was != at(seq))
                        && store.places.last_drawn(&id) == Some(frame);
                    let taken = store.places.at(&at(seq)).is_some_and(|other| {
                        *other != id && store.places.last_drawn(other) == Some(frame)
                    });
                    drop(store);
                    if !elsewhere && !taken {
                        prose.drawn_as(&id, at(seq), "words");
                        last_at.insert(id, at(seq));
                    }
                }
                Done::LetGo { loaded } => {
                    prose.let_go(&replies_at(loaded.iter().copied()), None);
                    last_at.retain(|_, place: &mut TextAt| loaded.contains(&place.seq().unwrap_or(0)));
                }
            }
        }

        let store = prose.0.borrow();
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
