use gpui_kit::{ListOffset, TestAppContext, px};

use super::Document;

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
