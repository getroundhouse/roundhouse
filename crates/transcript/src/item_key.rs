//! What each item the transcript shows is, whichever state it's in. GPUI keeps an element's state,
//! such as a text view's selection, under the element's ID, so an item keeps one key from pending to
//! recorded, and every element ID for an item comes from its key here.

use gpui_kit::ElementId;
use protocol::model::{AttachmentId, ClientMessageId, MessageId, Seq, ToolUseId};

use crate::task_lines::TaskGroup;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ItemKey {
    /// A prompt, by the ID the client that sent it chose, pending or recorded.
    Prompt(ClientMessageId),
    /// A block of the agent's output, by its message and its place in that message.
    Block(MessageId, u32),
    /// Anything else, by its event and its place in that event.
    Event(Seq, usize),
    /// A file a prompt carries, by the ID its client chose, pending or recorded.
    Attachment(AttachmentId),
    /// A call drawn from a record rather than an event, such as a subagent's newest, by its ID.
    Call(ToolUseId),
    /// What a subagent wrote just before a call, drawn from its record, by the call's ID.
    Before(ToolUseId),
    /// What a subagent has written since a call, drawn from its record: since its newest, or since the one that
    /// started it, if it's made none. By that call's ID.
    After(ToolUseId),
    /// The line for several background tasks of one group, at the chat's end.
    Tasks(TaskGroup),
}

impl ItemKey {
    /// A recorded prompt keeps the key it had while pending. One sent without an ID is its event.
    pub(crate) fn prompt(seq: Seq, client_msg_id: Option<&ClientMessageId>) -> Self {
        client_msg_id.map_or(ItemKey::Event(seq, 0), |id| ItemKey::Prompt(id.clone()))
    }

    /// A recorded block keeps the key it had while it streamed, when its event says where it sits in its
    /// message. Otherwise it's known by its place in its event.
    pub(crate) fn recorded_block(
        seq: Seq,
        position: usize,
        message: Option<&MessageId>,
        first_index: Option<u32>,
    ) -> Self {
        let offset = u32::try_from(position).ok();
        let index = first_index.zip(offset).and_then(|(first, offset)| first.checked_add(offset));
        match (message, index) {
            (Some(message), Some(index)) => ItemKey::Block(message.clone(), index),
            _ => ItemKey::Event(seq, position),
        }
    }

    /// The ID of one part of the item's element, such as its text or its output. Spelled out in
    /// full, so no two items or parts can share one.
    pub(crate) fn element(&self, part: &str) -> ElementId {
        let name = match self {
            ItemKey::Prompt(id) => format!("{part}/prompt/{id}"),
            ItemKey::Block(message, index) => format!("{part}/block/{message}/{index}"),
            ItemKey::Event(seq, position) => format!("{part}/event/{seq}/{position}"),
            ItemKey::Attachment(id) => format!("{part}/attachment/{id}"),
            ItemKey::Call(id) => format!("{part}/call/{id}"),
            ItemKey::Before(id) => format!("{part}/before/{id}"),
            ItemKey::After(id) => format!("{part}/after/{id}"),
            ItemKey::Tasks(group) => format!("{part}/tasks/{}", group.name()),
        };
        ElementId::Name(name.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_prompt_keeps_its_id_from_pending_to_recorded() {
        let id = ClientMessageId::from("p1");

        let pending = ItemKey::Prompt(id.clone()).element("prompt");
        let recorded = ItemKey::prompt(7, Some(&id)).element("prompt");

        assert_eq!(pending, recorded);
    }

    #[test]
    fn a_block_keeps_its_id_from_streaming_to_recorded() {
        let message = MessageId::from("msg_1");

        let streaming = ItemKey::Block(message.clone(), 2).element("text");
        let recorded = ItemKey::recorded_block(9, 0, Some(&message), Some(2)).element("text");

        assert_eq!(streaming, recorded);
        assert_eq!(ItemKey::recorded_block(9, 1, None, Some(2)), ItemKey::Event(9, 1), "nothing streamed");
    }

    #[test]
    fn no_two_items_or_parts_share_an_id() {
        let ids = [
            ItemKey::Event(1, 256).element("text"),
            ItemKey::Event(2, 0).element("text"),
            ItemKey::Event(1, 0).element("text"),
            ItemKey::Event(1, 0).element("output"),
            ItemKey::Block("a".into(), 0).element("text"),
            ItemKey::Block("b".into(), 0).element("text"),
            ItemKey::Block("a".into(), 1).element("text"),
            ItemKey::Block("1".into(), 0).element("text"),
            ItemKey::prompt(3, None).element("text"),
            ItemKey::Prompt("1".into()).element("text"),
            ItemKey::Attachment("1".into()).element("text"),
            ItemKey::Call("1".into()).element("text"),
            ItemKey::Before("1".into()).element("text"),
            ItemKey::After("1".into()).element("text"),
            ItemKey::Tasks(TaskGroup::Subagents).element("text"),
            ItemKey::Tasks(TaskGroup::Commands).element("text"),
        ];

        for (i, id) in ids.iter().enumerate() {
            assert!(!ids[i + 1..].contains(id), "{id:?} is shared");
        }
    }
}
