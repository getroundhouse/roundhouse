use gpui_kit::TestAppContext;
use protocol::event::{ContentBlock, EventBody};

use super::{Among, Of, Place, TextAt, TextKey, Texts, Written};
use crate::fixtures::{event, log};
use crate::selection::Selection;

fn at(seq: u64) -> TextAt {
    TextAt::only(seq, 0)
}

/// A reply, for a log whose row is loaded.
fn words() -> EventBody {
    let blocks = vec![ContentBlock::Text { text: "words".into(), images: Vec::new() }];
    EventBody::Assistant { message_id: None, parent_tool_use_id: None, model: None, blocks, first_index: None }
}

#[test]
fn the_subagent_tab_s_head_comes_before_every_row_s_texts_and_the_end_of_the_chat_and_its_foot_after() {
    let (head, foot) = (TextAt::first_of(Of::Head), TextAt::first_of(Of::Foot));
    let rows = [TextAt::only(0, 0), TextAt::only(u64::MAX, usize::MAX)];
    let end = [TextAt::at_the_end(0), TextAt::at_the_end(usize::MAX)];

    assert!(rows.iter().all(|row| head < *row && end.iter().all(|end| row < end)), "{head:?} {rows:?} {end:?}");
    assert!(end.iter().all(|end| *end < foot), "{end:?} {foot:?}");
}

#[test]
fn a_text_whose_event_is_let_go_of_is_kept_while_any_of_it_is_selected() {
    let texts = Texts::alone();
    for seq in 1..=3 {
        texts.drawn_at(at(seq), "words");
    }
    let held = log([event(3, words())]);

    texts.let_go(&held, Some(&(at(2)..=at(3))));
    let kept = [1, 2, 3].map(|seq| texts.keeps(&TextKey::of(at(seq))));
    texts.let_go(&held, None);

    assert_eq!(kept, [false, true, true], "what's loaded, and what's selected");
    assert!(!texts.keeps(&TextKey::of(at(2))), "until it isn't");
}

#[test]
fn the_subagent_tab_s_head_and_foot_are_kept_whatever_the_log_lets_go_of() {
    let texts = Texts::alone();
    let (head, foot) = (TextAt::first_of(Of::Head), TextAt::first_of(Of::Foot));
    texts.drawn_at(head, "What it was asked");
    texts.drawn_at(foot, "What it reported");

    texts.let_go(&log([]), None);

    assert!(texts.keeps(&TextKey::of(head)) && texts.keeps(&TextKey::of(foot)));
}

/// Draws each of `names` as the next text the `block`th block of the event at 1 nests, in one drawing of its row, in
/// a frame of its own.
fn nests(texts: &Texts, selection: &Selection, block: usize, names: &[&'static str]) {
    texts.new_frame();
    texts.drawing(Of::Event(1), || {
        for name in names {
            let place = Place::NextNested { seq: 1, block };
            texts.draw(selection, &Written { at: place, ..Written::unfound(at(1), named(name).0, "words") }, None);
        }
    });
}

fn named(name: &'static str) -> TextKey {
    TextKey::named(name)
}

fn nested(block: usize, n: usize) -> TextAt {
    TextAt { of: Of::Event(1), block, among: Among::Nested(n) }
}

/// A nested text is placed after those its block has drawn so far in this drawing of the row, block by block, and from
/// the first again each time the row is drawn.
#[gpui_kit::test]
fn a_nested_text_is_placed_after_those_its_block_has_drawn_so_far(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let selection = Selection::new(texts.clone(), cx.update(|cx| cx.focus_handle()));

    texts.new_frame();
    texts.drawing(Of::Event(1), || {
        for (block, name) in [(2, "first"), (2, "second"), (3, "other block's")] {
            let place = Place::NextNested { seq: 1, block };
            texts.draw(&selection, &Written { at: place, ..Written::unfound(at(1), named(name).0, "words") }, None);
        }
    });
    let once = ["first", "second", "other block's"].map(|name| texts.place_of(&named(name)));
    nests(&texts, &selection, 2, &["first", "second"]);
    let again = ["first", "second"].map(|name| texts.place_of(&named(name)));

    assert_eq!(once, [nested(2, 0), nested(2, 1), nested(3, 0)].map(Some), "each block counts its own");
    assert_eq!(again, [nested(2, 0), nested(2, 1)].map(Some), "and from the first again each time the row is drawn");
}

/// A nested text's place is its turn among those drawn, so one that's no longer drawn has none, rather than staying
/// hidden at a place after one that came after it, where a selection ending in it would take in the other.
#[gpui_kit::test]
fn a_nested_text_that_is_no_longer_drawn_has_no_place(cx: &mut TestAppContext) {
    let texts = Texts::alone();
    let selection = Selection::new(texts.clone(), cx.update(|cx| cx.focus_handle()));
    nests(&texts, &selection, 0, &["a thought", "another", "words"]);

    nests(&texts, &selection, 0, &["words"]);
    let closed = ["a thought", "another", "words"].map(|name| texts.place_of(&named(name)));
    texts.let_go(&log([event(1, words())]), None);
    let kept = texts.keeps(&named("another"));
    nests(&texts, &selection, 0, &["a thought", "another", "words"]);
    let opened = ["a thought", "another", "words"].map(|name| texts.place_of(&named(name)));

    assert_eq!(closed, [None, None, Some(nested(0, 0))]);
    assert!(kept, "kept while its row is loaded, to draw at once when it's shown again");
    assert_eq!(opened, [nested(0, 0), nested(0, 1), nested(0, 2)].map(Some));
}
