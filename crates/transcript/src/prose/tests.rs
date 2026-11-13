use protocol::event::{ContentBlock, EventBody};

use super::{Of, Prose, ProseId, TextAt};
use crate::fixtures::{event, log};

fn at(seq: u64) -> TextAt {
    TextAt::only(seq, 0)
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
