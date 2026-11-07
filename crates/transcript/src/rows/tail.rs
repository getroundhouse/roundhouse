//! Text the agent is still writing.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::text::TextView;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, ParentElement, SharedString, Styled, div};
use protocol::model::MessageId;
use protocol::tail::{BlockKind, Tail};

use super::{empty, row};

/// Text the agent is still writing. Tool input is hidden until it's complete, because it's partial JSON.
pub(super) fn render_tail(tail: &Tail, cx: &App) -> AnyElement {
    if tail.is_empty() {
        return empty();
    }
    let mut out = row();
    for message in &tail.messages {
        for block in &message.blocks {
            out = match block.kind {
                BlockKind::Text => {
                    let id = block_id(&message.message_id, block.index);
                    out.child(TextView::markdown(id, SharedString::from(block.text.clone())).selectable(true))
                }
                BlockKind::Thinking => out.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("Thinking… {}", last_line(&block.text))),
                ),
                BlockKind::ToolUse => out.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{}…", block.tool_name.as_deref().unwrap_or("Tool"))),
                ),
                BlockKind::Unknown => out,
            };
        }
    }
    out.into_any_element()
}

/// The kit's `TextView` keeps its state under this ID, including the text it shows. Blocks of messages streaming at
/// once would share that state if their IDs matched, and a block's ID has to hold still for as long as it streams.
fn block_id(message_id: &MessageId, index: u32) -> ElementId {
    ElementId::NamedInteger(format!("tail-{message_id}").into(), u64::from(index))
}

pub(super) fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").chars().take(160).collect()
}

fn last_line(text: &str) -> String {
    let line = text.lines().last().unwrap_or("");
    line.chars().rev().take(120).collect::<Vec<_>>().into_iter().rev().collect()
}

#[cfg(test)]
mod tests {
    use protocol::model::MessageId;

    use super::block_id;

    #[test]
    fn each_block_of_each_open_message_has_its_own_id() {
        let (first, second) = (MessageId::from("msg_a"), MessageId::from("msg_b"));
        assert_ne!(block_id(&first, 0), block_id(&second, 0), "messages whose IDs are the same length");
        assert_ne!(block_id(&first, 0), block_id(&first, 1), "blocks of one message");
        assert_eq!(block_id(&first, 0), block_id(&first, 0), "a block's ID holds still while it streams");
    }
}
