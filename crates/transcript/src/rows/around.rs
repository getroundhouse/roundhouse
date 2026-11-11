//! What drawing one event needs from the rest of the log and the model, read while they're held, so nothing is held
//! while it's drawn.

use client_core::session_log::{PromptState, SessionLog};
use gpui_kit::Entity;
use gpui_kit::component::text::TextViewState;
use protocol::event::{ContentBlock, EventBody};
use protocol::model::Seq;

use crate::attachments::{self, Carried};
use crate::calls::agent;
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::transcript::Model;

mod nearby;
mod reply;
mod subagent;

pub(super) use reply::{Answer, Reply};
pub(crate) use subagent::SubagentCalls;
pub(super) use subagent::{Subagent, drawn_in_its_call};

/// How many events apart a call and its result can be and still be drawn together. A result follows its call
/// within a few events. One further away is drawn where it is.
const NEAR: usize = 64;

pub(super) struct Around {
    /// What each of the event's blocks needs, by position.
    pub(super) blocks: Vec<BlockAround>,
    /// For an approval: whether it's still waiting.
    pub(super) approval_open: bool,
    /// For a prompt: the files it carries.
    pub(super) carried: Vec<Carried>,
    /// For a prompt: whether the agent has it.
    pub(super) prompt: PromptState,
    /// For a prompt: its text view's state, if the transcript keeps it.
    pub(super) prompt_text: Option<Entity<TextViewState>>,
}

/// What drawing one block of an event needs.
pub(super) struct BlockAround {
    /// What the block is, wherever it's drawn.
    pub(super) key: ItemKey,
    pub(super) open: bool,
    /// For a call: what's come back for it. For a result drawn on its own: itself.
    pub(super) reply: Reply,
    /// For a call that handed work to a subagent: what the subagent has done.
    pub(super) subagent: Option<Subagent>,
    /// For a result: whether it's drawn with its call instead.
    pub(super) with_call: bool,
    /// For text: its text view's state, if the transcript keeps it.
    pub(super) text: Option<Entity<TextViewState>>,
}

impl Around {
    pub(super) fn read(log: &SessionLog, m: &Model, seq: Seq, body: &EventBody) -> Self {
        let blocks = blocks_of(body)
            .iter()
            .enumerate()
            .map(|(position, block)| BlockAround::read(log, m, seq, block, block_key(seq, body, position)))
            .collect();
        let mut around =
            Around { blocks, approval_open: false, carried: Vec::new(), prompt: PromptState::Taken, prompt_text: None };
        if let EventBody::UserPrompt { attachments, client_msg_id, .. } = body {
            around.carried = attachments::carried(m, attachments);
            around.prompt = log.prompt_state(seq);
            around.prompt_text = m.long_texts.get(&ItemKey::prompt(seq, client_msg_id.as_ref())).cloned();
        }
        if let EventBody::ApprovalRequested { approval_id, .. } = body {
            around.approval_open = log
                .open_approvals()
                .iter()
                .any(|e| matches!(&e.body, EventBody::ApprovalRequested { approval_id: id, .. } if id == approval_id));
        }
        around
    }

    /// Every image the event draws now, for its row to ask the hub for: a prompt's, and those of each call or result
    /// that's open, including what a call gathered from its subagent. A closed call's images wait until it's opened.
    pub(super) fn images(&self) -> Vec<&Carried> {
        let mut images: Vec<&Carried> = self.carried.iter().collect();
        for block in self.blocks.iter().filter(|block| block.open) {
            images.extend(block.reply.result().into_iter().flat_map(Answer::images));
            let gathered = block.subagent.iter().flat_map(|subagent| subagent.events.iter().flatten());
            images.extend(gathered.flat_map(|(_, around)| around.images()));
        }
        images
    }
}

impl BlockAround {
    fn read(log: &SessionLog, m: &Model, seq: Seq, block: &ContentBlock, key: ItemKey) -> Self {
        let open = m.folds.is_open(&key, Fold::of(block));
        // Drawn with its call when the call is close enough before it to have found it.
        let with_call = matches!(block, ContentBlock::ToolResult { tool_use_id, .. }
            if nearby::call_before(log, seq, tool_use_id.as_str(), NEAR).is_some());
        let (reply, subagent) = match block {
            ContentBlock::ToolUse { id, name, .. } => {
                (Reply::to(log, m, seq, id), agent::hands_off(name).then(|| Subagent::read(log, m, seq, id, open)))
            }
            ContentBlock::ToolResult { .. } if !with_call => (Reply::of(block, m), None),
            ContentBlock::ToolResult { .. }
            | ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Unknown => (Reply::Nothing, None),
        };
        let text = m.long_texts.get(&key).cloned();
        Self { key, open, reply, subagent, with_call, text }
    }
}

/// The blocks an event holds, if it's one that holds any.
pub(super) fn blocks_of(body: &EventBody) -> &[ContentBlock] {
    if let EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. } = body { blocks } else { &[] }
}

/// What the block at `position` in an event is, wherever it's drawn. One the agent wrote keeps the key it had
/// while it streamed.
pub(super) fn block_key(seq: Seq, body: &EventBody, position: usize) -> ItemKey {
    if let EventBody::Assistant { message_id, first_index, .. } = body {
        return ItemKey::recorded_block(seq, position, message_id.as_ref(), *first_index);
    }
    ItemKey::Event(seq, position)
}

#[cfg(test)]
mod tests {
    use protocol::attachment::{Attachment, MediaType};
    use protocol::event::{ContentBlock, EventBody};

    use super::{Around, Reply, block_key};
    use crate::fixtures::{calls, calls_to, event, from_the_agent, log, result, within};
    use crate::item_key::ItemKey;
    use crate::transcript::Model;

    #[test]
    fn a_recorded_block_is_known_by_its_place_in_its_message() {
        let finished = |first_index| EventBody::Assistant {
            message_id: Some("m".into()),
            parent_tool_use_id: None,
            model: None,
            blocks: vec![ContentBlock::Text { text: "a".into(), images: Vec::new() }],
            first_index,
        };

        assert_eq!(block_key(7, &finished(Some(2)), 0), ItemKey::Block("m".into(), 2));
        assert_eq!(block_key(7, &finished(None), 0), ItemKey::Event(7, 0), "nothing streamed");
    }

    #[test]
    fn calls_made_at_once_each_get_their_own_result() {
        let log = log([event(1, calls(&["a", "b"])), event(2, result("b")), event(3, result("a"))]);
        let m = Model::default();

        let around = Around::read(&log, &m, 1, &calls(&["a", "b"]));

        let found: Vec<Option<String>> =
            around.blocks.iter().map(|b| b.reply.result().map(|a| a.text.clone())).collect();
        assert_eq!(found, [Some("from a".to_string()), Some("from b".to_string())]);
        assert!(Around::read(&log, &m, 2, &result("b")).blocks[0].with_call);
        assert!(Around::read(&log, &m, 3, &result("a")).blocks[0].with_call);
    }

    /// The result of the call `id`, with an image.
    fn with_an_image(id: &str) -> EventBody {
        let mut shown = result(id);
        if let EventBody::AgentUser { blocks, .. } = &mut shown
            && let [ContentBlock::ToolResult { images, .. }] = blocks.as_mut_slice()
        {
            let size_bytes = 1;
            images.push(Attachment { id: "i1".into(), name: "image".into(), media_type: MediaType::Png, size_bytes });
        }
        shown
    }

    #[test]
    fn a_call_s_images_are_fetched_only_once_it_s_open() {
        let log = log([event(1, calls(&["a"])), event(2, with_an_image("a"))]);
        let mut m = Model::default();
        assert!(Around::read(&log, &m, 1, &calls(&["a"])).images().is_empty(), "closed");

        m.folds.choose(ItemKey::Event(1, 0), true);

        assert_eq!(Around::read(&log, &m, 1, &calls(&["a"])).images().len(), 1, "open");
    }

    #[test]
    fn a_subagent_s_images_are_fetched_with_the_call_that_started_it() {
        let agent = calls_to("Agent", &["a"]);
        let log = log([
            event(1, agent.clone()),
            event(2, within("a", calls(&["b"]))),
            event(3, within("a", with_an_image("b"))),
        ]);
        let mut m = Model::default();
        m.folds.choose(ItemKey::Event(2, 0), true);
        assert!(Around::read(&log, &m, 1, &agent).images().is_empty(), "the agent's call is closed");

        m.folds.choose(ItemKey::Event(1, 0), true);

        assert_eq!(Around::read(&log, &m, 1, &agent).images().len(), 1, "opened, it gathers its subagent's");
    }

    #[test]
    fn a_result_from_the_agent_is_its_call_s_result_too() {
        let sent = from_the_agent(result("a"));
        let log = log([event(1, calls_to("advisor", &["a"])), event(2, sent.clone())]);
        let m = Model::default();

        let call = Around::read(&log, &m, 1, &calls_to("advisor", &["a"]));

        assert_eq!(call.blocks[0].reply.result().map(|answer| answer.text.as_str()), Some("from a"));
        assert!(Around::read(&log, &m, 2, &sent).blocks[0].with_call, "so it isn't drawn twice");
    }

    #[test]
    fn a_call_the_log_says_was_dropped_has_no_result_unless_one_came_after_all() {
        let dropped = EventBody::ToolCallDropped { tool_use_id: "a".into() };
        let gone = log([event(1, calls(&["a", "b"])), event(2, dropped.clone())]);
        let came = log([event(1, calls(&["a"])), event(2, dropped), event(3, result("a"))]);
        let m = Model::default();

        let beside = Around::read(&gone, &m, 1, &calls(&["a", "b"]));
        let late = Around::read(&came, &m, 1, &calls(&["a"]));

        assert!(matches!(beside.blocks[0].reply, Reply::Dropped));
        assert!(matches!(beside.blocks[1].reply, Reply::Nothing), "the call made beside it is still waiting");
        assert_eq!(late.blocks[0].reply.result().map(|answer| answer.text.as_str()), Some("from a"), "it stands");
    }

    #[test]
    fn a_result_without_its_call_nearby_is_drawn_where_it_is() {
        let log = log([event(1, calls(&["a"])), event(2, result("elsewhere"))]);
        let m = Model::default();

        assert!(!Around::read(&log, &m, 2, &result("elsewhere")).blocks[0].with_call);
        let call = Around::read(&log, &m, 1, &calls(&["a"]));
        assert!(
            matches!(call.blocks[0].reply, Reply::Nothing),
            "a call whose result hasn't come back has nothing under it"
        );
    }
}
