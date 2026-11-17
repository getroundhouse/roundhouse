//! That the normalizer types each call and what came back for it, on each way a line can come to it.

use protocol::attachment::AttachmentBytes;
use protocol::capability::calls::call::Read;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, EventBody};
use serde_json::json;

use super::{PNG, from_the_agent, read_an_image};
use crate::claude::normalize::{Normalizer, Parsed};

/// What each call and result in what `lines` come to says, in order, through a normalizer, which is what types them:
/// each call, and what came back for each result.
fn typed(lines: &[String]) -> (Vec<Call>, Vec<CallOutput>) {
    let mut normalizer = Normalizer::default();
    let (mut calls, mut outputs) = (Vec::new(), Vec::new());
    for line in lines {
        let (Parsed::Event(body) | Parsed::EventWithImages { body, .. }) = normalizer.line(line).parsed else {
            continue;
        };
        let (EventBody::Assistant { blocks, .. } | EventBody::AgentUser { blocks, .. }) = body else { continue };
        for block in blocks {
            match block {
                ContentBlock::Call { call, .. } => calls.push(call),
                ContentBlock::CallResult { output, .. } => outputs.push(output),
                ContentBlock::Text { .. }
                | ContentBlock::Thinking { .. }
                | ContentBlock::Image { .. }
                | ContentBlock::ToolUse { .. }
                | ContentBlock::ToolResult { .. }
                | ContentBlock::Unknown => {}
            }
        }
    }
    (calls, outputs)
}

/// Its call and its result both come from the agent, in lines of one message.
#[test]
fn the_advisor_s_call_and_its_result_are_the_advisor_s() {
    let call = json!({"type":"server_tool_use","id":"srvtoolu_1","name":"advisor","input":{}});
    let result = json!({"type":"advisor_tool_result","tool_use_id":"srvtoolu_1",
        "content":{"type":"advisor_result","text":"Test the parser first."}});

    let said = typed(&[from_the_agent(&call), from_the_agent(&result)]);

    assert_eq!(said, (vec![Call::Advisor], vec![CallOutput::Words { text: "Test the parser first.".into() }]));
}

/// A result with an image goes to the hub another way than one without, and is read against its call all the same.
#[test]
fn a_result_that_names_an_image_is_read_against_the_call_it_answers() {
    let read = json!({"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"a.png"}});
    let base64 = AttachmentBytes::new(PNG.to_vec()).to_base64();

    let said = typed(&[from_the_agent(&read), read_an_image(&base64)]);

    let made = Call::Read(Read { path: "a.png".into(), lines: None, pages: None });
    assert_eq!(said, (vec![made], vec![CallOutput::Words { text: "a\nb".into() }]));
}
