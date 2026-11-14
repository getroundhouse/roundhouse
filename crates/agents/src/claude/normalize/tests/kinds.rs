//! That the normalizer says what kind of call each call and result is, on each way a line can come to it.

use protocol::attachment::AttachmentBytes;
use protocol::event::{CallKind, ContentBlock, EventBody};
use serde_json::json;

use super::{PNG, from_the_agent, read_an_image};
use crate::claude::normalize::{Normalizer, Parsed};

/// The kinds of the calls and results in what `lines` come to, in order, through a normalizer, which is what says
/// them.
fn kinds_said(lines: &[String]) -> Vec<Option<CallKind>> {
    let mut normalizer = Normalizer::default();
    let mut said = Vec::new();
    for line in lines {
        let (Parsed::Event(body) | Parsed::EventWithImages { body, .. }) = normalizer.line(line).parsed else {
            continue;
        };
        let (EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. }) = body else { continue };
        said.extend(blocks.into_iter().filter_map(|block| match block {
            ContentBlock::ToolUse { call, .. } | ContentBlock::ToolResult { call, .. } => Some(call),
            ContentBlock::Text { .. }
            | ContentBlock::Thinking { .. }
            | ContentBlock::Image { .. }
            | ContentBlock::Call { .. }
            | ContentBlock::CallResult { .. }
            | ContentBlock::Unknown => None,
        }));
    }
    said
}

/// Its call and its result both come from the agent, in lines of one message, and each says it's the advisor's.
#[test]
fn the_advisor_s_call_and_its_result_say_what_kind_of_call_it_is() {
    let call = json!({"type":"server_tool_use","id":"srvtoolu_1","name":"advisor","input":{}});
    let result = json!({"type":"advisor_tool_result","tool_use_id":"srvtoolu_1",
        "content":{"type":"advisor_result","text":"Test the parser first."}});

    let said = kinds_said(&[from_the_agent(&call), from_the_agent(&result)]);

    assert_eq!(said, [Some(CallKind::Advisor), Some(CallKind::Advisor)]);
}

/// A result with an image goes to the hub another way than one without, and says its kind all the same.
#[test]
fn a_result_that_names_an_image_says_what_kind_of_call_it_answers() {
    let read = json!({"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"a.png"}});
    let base64 = AttachmentBytes::new(PNG.to_vec()).to_base64();

    let said = kinds_said(&[from_the_agent(&read), read_an_image(&base64)]);

    assert_eq!(said, [Some(CallKind::Read), Some(CallKind::Read)]);
}
