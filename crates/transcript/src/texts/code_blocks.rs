//! What a code block in a text has at its top right: its language, where the kit would draw it, and before it, while
//! the block is hovered, a button that copies its code. The kit draws a block's actions in place of its language, so
//! the actions draw the language too.

use gpui_kit::base::text::editor_host::{CODE_BLOCK_GROUP, CodeBlock, RenderContext};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels, Styled,
    TestSupportExt as _, Window, div, px,
};
use look::Typeset as _;

/// The room above a block's code, which the kit gives a block whose language it draws, and every block has, so the
/// language and the button sit above the code rather than over it.
const ROOM: Pixels = px(28.);

/// How tall the language's line is, which the button is centred on. The kit draws the actions 1px lower than it draws
/// the language, on a line 2px shorter, so the language's middle is where the kit's is.
const LINE: Pixels = px(14.);

/// `context`, with each code block's actions and the room above its code for them.
pub(super) fn with_actions(context: RenderContext) -> RenderContext {
    let style = context.style();
    let roomy = style.clone().with_code_block(style.code_block().clone().pt(ROOM));
    context.with_style(roomy).with_code_block_actions(actions)
}

/// The button that copies `block`'s code, shown while the block is hovered, then its language, if it has one.
fn actions(block: &CodeBlock, _: &mut Window, cx: &mut App) -> AnyElement {
    let copy = look::copy_button("copy", block.code(), "Copy code", cx);
    // Centred on the line without making it taller, so the line sits where the kit's language would.
    let copy = div().flex().items_center().h_0().mr(look::Space::XSmall).child(copy);
    let copy = copy.invisible().group_hover(CODE_BLOCK_GROUP, Styled::visible);
    h_flex()
        .id("code-block-actions")
        // As far in from the block's edge as the kit draws the language.
        .pr(look::Space::XSmall)
        .line_height(LINE)
        .typeset(look::Type::Detail)
        .font_family(look::Type::Detail.family(cx))
        .text_color(look::colors(cx).text_muted)
        // A line's height, so a block with no language has the line too.
        .child(div().w_0().child("\u{a0}"))
        .child(copy)
        .children(block.lang().filter(|lang| !lang.is_empty()))
        .test_support()
        .into_any_element()
}
