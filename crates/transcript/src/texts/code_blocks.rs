//! What a code block in a text has above its code: its language at the top left, and at the top right, while the block
//! is hovered, a button that copies its code. The kit draws a block's actions in place of its language, so the app
//! draws the language itself, over the block it wraps. Under touch nothing hovers it, so a long press on the block
//! copies it instead.

use gpui_kit::base::text::editor_host::{CODE_BLOCK_GROUP, CodeBlock, RenderContext};
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels, Styled,
    TestSupportExt as _, Window, div, px,
};
use look::{RightClickMenu as _, Typeset as _};

/// The room above a block's code, which the kit gives a block whose language it draws, and every block has, so the
/// language and the button sit above the code rather than over it.
const ROOM: Pixels = px(28.);

/// How tall the line the language and the button are on is, which the button is centred on, over the size of the
/// language, a detail: 14px at a desk.
const LINE: f32 = 1.17;

/// How far down that line is from the block's top, where the kit draws a block's actions. With [`LINE`], at a desk its
/// middle is where the middle of the kit's own line for a language is, which is 2px taller and 1px higher.
const LINE_TOP: Pixels = px(8.);

/// How far in the code is from the block's left edge, which is the kit's padding for it, so the language starts where
/// the code does.
const CODE_LEFT: Pixels = px(16.);

/// `context`, with each code block's language, its actions and the room above its code for them.
pub(super) fn with_actions(context: RenderContext) -> RenderContext {
    let style = context.style();
    let code_block = style.code_block().clone().pt(ROOM).line_height(look::CodeLines::Chat.height());
    let roomy = style.clone().with_code_block(code_block);
    context.with_style(roomy).with_code_block_actions(actions).with_code_block_wrap(|block, drawn, _, cx| {
        let language = block.lang().filter(|lang| !lang.is_empty()).map(|lang| {
            line(cx).id("code-block-language").absolute().top(LINE_TOP).left(CODE_LEFT).child(lang).test_support()
        });
        drawn.children(language).test_support().copied_by_a_long_press(block.code(), cx)
    })
}

/// What a code block that isn't one of a text's blocks is drawn with, such as a diagram's that couldn't be drawn, as
/// the text's own are: with its language and the button that copies its code.
pub(crate) fn code_context(cx: &App) -> RenderContext {
    with_actions(RenderContext::from_app(cx))
}

/// The button that copies `block`'s code, shown while the block is hovered. Under touch, nothing.
fn actions(block: &CodeBlock, _: &mut Window, cx: &mut App) -> AnyElement {
    let copy = look::copy_button_shown_while_over(CODE_BLOCK_GROUP, "copy", block.code(), "Copy code", cx);
    // Centred on the line without making it taller, so the line sits where the language's does.
    let copy = copy.map(|copy| div().flex().items_center().h_0().child(copy));
    line(cx)
        .id("code-block-actions")
        // As far in from the block's edge as the kit draws its language.
        .pr(look::Space::XSmall)
        // A line's height, so the button is centred on it.
        .child(div().w_0().child("\u{a0}"))
        .children(copy)
        .test_support()
        .into_any_element()
}

/// The line above a block's code, set as the language is.
fn line(cx: &App) -> gpui_kit::Div {
    h_flex()
        .line_height(look::Type::Detail.lines(LINE))
        .typeset(look::Type::Detail)
        .font_family(look::Type::Detail.family(cx))
        .text_color(look::colors(cx).text_muted)
}
