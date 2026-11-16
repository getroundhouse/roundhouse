// Changed in Roundhouse's copy of gpui-kit, as third_party/README.md lists.
#[cfg(feature = "gpui-fast")]
use gpui_fast as gpui;

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div,
};
use gpui_base::{
    TextViewStyle,
    text::editor_host::{
        BlockNode, CODE_BLOCK_GROUP, FlowPosition, LinkMark, RenderContext, parse_markdown,
    },
};

struct ExternalText {
    blocks: Vec<(u64, BlockNode)>,
    context: RenderContext,
}

impl Render for ExternalText {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let blocks: Vec<_> = self.blocks.iter().map(|(_, block)| block).collect();
        div().children(self.blocks.iter().enumerate().map(|(ix, (id, block))| {
            block.render_with_id(
                ("external-block", *id),
                FlowPosition::at(&blocks, ix, ix + 1 == blocks.len()),
                &self.context,
                window,
                cx,
            )
        }))
    }
}

/// Compile and render as an outside consumer: no private node/context access.
#[gpui::test]
fn external_host_uses_local_rendering_services(cx: &mut TestAppContext) {
    let highlights = Arc::new(AtomicUsize::new(0));
    let images = Arc::new(AtomicUsize::new(0));
    let context = RenderContext::new(TextViewStyle::default())
        .with_code_block_highlighter({
            let highlights = highlights.clone();
            move |block| {
                assert_eq!(block.lang().as_deref(), Some("rust"));
                highlights.fetch_add(1, Ordering::SeqCst);
                Vec::new()
            }
        })
        .with_image_source({
            let images = images.clone();
            move |uri| {
                images.fetch_add(1, Ordering::SeqCst);
                uri.clone().into()
            }
        })
        .on_link_click(|_, _, _, _| {})
        .with_link_reference(
            "target",
            LinkMark {
                url: "https://example.com".into(),
                ..Default::default()
            },
        );
    cx.update(gpui_base::init);
    let blocks = parse_markdown(
        "```rust\nfn main() {}\n```\n\n![image](test://image)\n\n[reference][target]\n\n[target]: https://example.com\n",
        Default::default(),
    )
    .unwrap();
    // Allocate identity once when importing this immutable fixture, not in render.
    let blocks = blocks
        .into_iter()
        .enumerate()
        .map(|(ix, block)| (ix as u64 + 100, block))
        .collect();
    let (_, vcx) = cx.add_window_view(move |_, _| ExternalText { blocks, context });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(highlights.load(Ordering::SeqCst) > 0);
    assert!(images.load(Ordering::SeqCst) > 0);
}

/// A host's code block actions are drawn for each code block, one in a list
/// too, and given the block they're for.
#[gpui::test]
fn external_host_draws_code_block_actions(cx: &mut TestAppContext) {
    let drawn = Arc::new(Mutex::new(Vec::new()));
    let context = RenderContext::new(TextViewStyle::default()).with_code_block_actions({
        let drawn = drawn.clone();
        move |block, _, _| {
            drawn.lock().unwrap().push(block.code().trim().to_string());
            div()
                .invisible()
                .group_hover(CODE_BLOCK_GROUP, |this| this.visible())
                .child("Copy")
        }
    });
    cx.update(gpui_base::init);
    let blocks = parse_markdown(
        "```rust\ntop\n```\n\n1. Item\n\n   ```\n   nested\n   ```\n",
        Default::default(),
    )
    .unwrap();
    let blocks = blocks
        .into_iter()
        .enumerate()
        .map(|(ix, block)| (ix as u64, block))
        .collect();
    let (_, vcx) = cx.add_window_view(move |_, _| ExternalText { blocks, context });
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let mut drawn = drawn.lock().unwrap().clone();
    drawn.sort();
    drawn.dedup();
    assert_eq!(drawn, ["nested", "top"]);
}
