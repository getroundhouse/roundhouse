//! What comes back for a call: its text, its images and what it changed, and the advisor's, which the agent sends.

use super::super::*;
use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, FileChange};
use serde_json::json;

/// The start of a PNG, which is enough for its bytes to say what it is.
pub(super) const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

/// A read of an image, as the CLI sends it: in the result, and again in its own record of the read.
pub(super) fn read_an_image(base64: &str) -> String {
    json!({"type":"user","message":{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"t1","content":[
            {"type":"text","text":"a"},
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":base64}},
            {"type":"text","text":"b"}
        ]
    }]},"tool_use_result":{"type":"image","file":{"base64":base64,"type":"image/png"}}})
    .to_string()
}

#[test]
fn a_tool_result_s_images_become_attachments_kept_apart_from_the_line() {
    let base64 = AttachmentBytes::new(PNG.to_vec()).to_base64();

    let Parsed::EventWithImages { body: EventBody::AgentUser { blocks, .. }, images, raw } =
        parse_line(&read_an_image(&base64))
    else {
        panic!()
    };

    let [ContentBlock::CallResult { output: CallOutput::Words { text }, images: named, .. }] = blocks.as_slice() else {
        panic!("{blocks:?}")
    };
    assert_eq!(text, "a\nb");
    let [kept] = images.as_slice() else { panic!("{images:?}") };
    assert_eq!(named, std::slice::from_ref(&kept.attachment), "the result names what's kept");
    assert_eq!(kept.attachment.media_type, MediaType::Png);
    assert_eq!(kept.data.as_slice(), PNG);
    assert!(!raw.contains(&base64), "the line keeps none of the image's bytes");
    let raw: Value = serde_json::from_str(&raw).unwrap();
    let marker = images::marker(&kept.attachment.id);
    assert_eq!(raw["message"]["content"][0]["content"][1]["source"]["data"], marker);
    assert_eq!(raw["tool_use_result"]["file"]["base64"], marker);
}

#[test]
fn an_image_block_that_isn_t_an_image_stays_in_the_line() {
    let not_an_image = AttachmentBytes::new(b"plain text".to_vec()).to_base64();

    for line in [read_an_image(&not_an_image), read_an_image("not base64!")] {
        let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line) else { panic!("{line}") };
        assert!(matches!(blocks.as_slice(), [ContentBlock::CallResult { images, .. }] if images.is_empty()));
    }
}

#[test]
fn a_result_written_for_the_model_is_still_its_call_s_result() {
    let line = json!({"type":"user","message":{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"t1","content":"Interrupted","is_error":true
    }]},"isSynthetic":true});

    let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line.to_string()) else { panic!() };

    assert!(matches!(blocks.as_slice(), [ContentBlock::CallResult { is_error: true, .. }]), "{blocks:?}");
}

/// Even when the call it answers isn't known, as when the CLI answers one from before it was started again, since the
/// CLI names the file.
#[test]
fn an_edit_s_result_carries_what_it_changed() {
    let line = json!({"type":"user","message":{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"t1","content":"The file has been updated."
    }]},"tool_use_result":{"filePath":"/w/a.rs","structuredPatch":[
        {"oldStart":3,"oldLines":1,"newStart":3,"newLines":1,"lines":["-old","+new"]}
    ]}})
    .to_string();

    let Parsed::Event(EventBody::AgentUser { blocks, .. }) = Normalizer::default().line(&line).parsed else { panic!() };

    let [ContentBlock::CallResult { output: CallOutput::Changed { files }, .. }] = blocks.as_slice() else {
        panic!("{blocks:?}")
    };
    let [file] = files.as_slice() else { panic!("{files:?}") };
    let Some(FileChange::Changed { hunks }) = &file.change else { panic!("{file:?}") };
    assert_eq!(file.path, "/w/a.rs");
    assert_eq!(hunks[0].new_start, 3);
    assert_eq!(hunks[0].lines.len(), 2);
}

#[test]
fn a_command_that_printed_nothing_has_an_empty_result() {
    let result = |content: &str, stdout: &str| {
        let line = json!({"type":"user","message":{"role":"user","content":[{
            "type":"tool_result","tool_use_id":"t1","content":content
        }]},"tool_use_result":{"stdout":stdout,"stderr":"","interrupted":false}})
        .to_string();
        let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line) else { panic!() };
        let [ContentBlock::CallResult { output: CallOutput::Words { text }, .. }] = blocks.as_slice() else {
            panic!("{blocks:?}")
        };
        text.clone()
    };

    assert_eq!(result("(Bash completed with no output)", ""), "");
    assert_eq!(result("Command running in background with ID: b1", ""), "Command running in background with ID: b1");
    assert_eq!(result("(Bash completed with no output)", "x"), "(Bash completed with no output)", "it printed");
}

/// A line from the agent with only `block`, as Claude sends each block of a message.
pub(super) fn from_the_agent(block: &Value) -> String {
    json!({"type":"assistant","message":{"id":"m","content":[block]}}).to_string()
}

#[test]
fn a_call_to_the_advisor_is_a_call_like_any_other() {
    let call = json!({"type":"server_tool_use","id":"srvtoolu_1","name":"advisor","input":{}});
    let start = json!({"type":"stream_event","api_message_id":"m","event":{
        "type":"content_block_start","index":2,"content_block":call
    }});

    let recorded = parse_line(&from_the_agent(&call));
    let streamed = parse_line(&start.to_string());

    let Parsed::Event(EventBody::Assistant { blocks, .. }) = recorded else { panic!("{recorded:?}") };
    assert_eq!(blocks, [ContentBlock::Call { id: "srvtoolu_1".into(), call: Call::Advisor }]);
    let Parsed::Delta(StreamDelta { change: DeltaChange::BlockStart { kind, .. }, .. }) = streamed else {
        panic!("{streamed:?}")
    };
    assert_eq!(kind, BlockKind::ToolUse);
}

#[test]
fn the_advisor_s_result_has_its_advice_only_when_it_can_be_read() {
    let result = |content: Value| {
        let block = json!({"type":"advisor_tool_result","tool_use_id":"srvtoolu_1","content":content});
        let parsed = parse_line(&from_the_agent(&block));
        let Parsed::Event(EventBody::Assistant { blocks, .. }) = parsed else { panic!("{parsed:?}") };
        let [ContentBlock::CallResult { tool_use_id, is_error, output, .. }] = blocks.as_slice() else {
            panic!("{blocks:?}")
        };
        assert_eq!(tool_use_id.as_str(), "srvtoolu_1");
        if let CallOutput::Words { text } = output {
            return (*is_error, text.clone(), false);
        }
        assert_eq!(*output, CallOutput::Encrypted, "advice is its words, or encrypted");
        (*is_error, String::new(), true)
    };
    let said = |text: &str| (false, text.to_string(), false);
    let failed = |text: &str| (true, text.to_string(), false);
    // Made up. A real one is an opaque string only the API can read.
    let encrypted = json!({"type":"advisor_redacted_result","encrypted_content":"bm90IHJlYWwgYWR2aWNl"});
    let refused = json!({"type":"advisor_redacted_result","encrypted_content":"eA==","stop_reason":"refusal"});
    let unavailable = json!({"type":"advisor_tool_result_error","error_code":"overloaded"});

    assert_eq!(result(encrypted), (false, String::new(), true));
    assert_eq!(result(json!({"type":"advisor_result","text":"Test it first."})), said("Test it first."));
    assert_eq!(result(unavailable), failed("overloaded"));
    assert_eq!(result(refused), failed("refusal"));
    // Nothing says these are encrypted, so they aren't taken to be.
    assert_eq!(result(json!({"type":"advisor_result_v2","text":"Newer."})), said("Newer."));
    assert_eq!(result(Value::Null), said(""));
}
