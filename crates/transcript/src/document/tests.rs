use gpui_kit::{ListOffset, TestAppContext, px};

use super::Document;
use super::changes::{Change, changed};
use crate::reveal::Wanted;
use crate::selection::Way;
use crate::step_keys::Steps as _;
use crate::texts::Of;

/// A file the agent changed is read again when its turn ends, and shown again under its name, keeping where it's
/// scrolled to, while another starts at its top.
#[gpui_kit::test]
fn a_text_shown_again_under_its_name_keeps_where_it_s_scrolled_to(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (document, cx) = cx.add_window_view(|_, cx| Document::new(cx));
    let paragraphs = |count: usize| "A paragraph.\n\n".repeat(count);
    let show = |cx: &mut gpui_kit::VisualTestContext, name: &'static str, text: String| {
        document.update(cx, |document, cx| document.show(name.into(), text.into(), cx));
        cx.run_until_parked();
    };
    let scrolled_to = |cx: &mut gpui_kit::VisualTestContext| {
        let top = document.read_with(cx, |document, _| document.list.logical_scroll_top());
        (top.item_ix, top.offset_in_item)
    };
    show(cx, "notes.md", paragraphs(200));
    document.update(cx, |document, _| document.list.scroll_to(ListOffset { item_ix: 120, offset_in_item: px(5.) }));

    show(cx, "notes.md", paragraphs(201));
    assert_eq!(scrolled_to(cx), (120, px(5.)), "read again, it stays where it was");
    show(cx, "other.md", paragraphs(200));
    assert_eq!(scrolled_to(cx), (0, px(0.)), "and another starts at its top");
}

/// A step to a heading takes over from what was being brought into view, such as the end of what's selected that a
/// key moved, which would scroll back to it after.
#[gpui_kit::test]
fn a_step_takes_over_from_what_was_being_brought_into_view(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let (document, cx) = cx.add_window_view(|_, cx| Document::new(cx));
    let text = "# One\n\nA paragraph.\n\n# Two\n\nAnother.";
    document.update(cx, |document, cx| document.show("notes.md".into(), text.into(), cx));
    cx.run_until_parked();
    document.update(cx, |document, _| document.selection.reveal().want(Wanted::End, Of::Document(3)));

    document.update(cx, |document, cx| document.step(Way::On, cx));

    let wanted = document.read_with(cx, |document, _| document.selection.reveal().wants_anything());
    assert!(!wanted, "it isn't brought into view after");
}

/// A changed text shows each block either version has once, in order: one both have as it is, and where they differ,
/// what was removed, then what was added. A list differs item by item, since one changed item shouldn't mark the rest.
#[test]
fn what_changed_shows_block_by_block_and_a_list_item_by_item() {
    let was = "# Notes\n\nOne.\n\n- x\n- y\n\n1. first\n2. second\n\nThe end.\n";
    let is = "# Notes\n\nOne, changed.\n\n- x\n- Y\n- z\n\n1. first\n2. second\n\nThe end.\n";

    let shown: Vec<(String, Change)> = changed(was, is)
        .into_iter()
        .map(|(block, change)| (block.source[..block.shown].trim().to_string(), change))
        .collect();

    let expected = [
        ("# Notes", Change::Kept),
        ("One.", Change::Removed),
        ("One, changed.", Change::Added),
        ("- x", Change::Kept),
        ("- y", Change::Removed),
        ("- Y", Change::Added),
        ("- z", Change::Added),
        ("1. first", Change::Kept),
        ("2. second", Change::Kept),
        ("The end.", Change::Kept),
    ];
    let expected: Vec<(String, Change)> =
        expected.into_iter().map(|(said, change)| (said.to_string(), change)).collect();
    assert_eq!(shown, expected);
}
