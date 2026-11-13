use protocol::attachment::{AttachmentBytes, MediaType};
use protocol::event::{ContentBlock, FileChange, TurnOutcome};
use protocol::fixtures;
use protocol::model::{PermissionMode, ToolUseId};
use serde_json::json;

use super::*;

mod kinds;

#[test]
fn output_that_is_not_json_is_kept_rather_than_failing() {
    assert_eq!(
        parse_line("Warning: something"),
        Parsed::Event(EventBody::AgentOther { kind: "unparsed".into(), parent_tool_use_id: None })
    );
}

#[test]
fn a_lone_surrogate_does_not_hide_an_approval() {
    // JSON.stringify writes half an emoji this way, and serde_json refuses it.
    let line = r#"{"type":"control_request","request_id":"r1","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"echo \ud83d"}}}"#;
    let Parsed::Event(EventBody::ApprovalRequested { approval_id, input, .. }) = parse_line(line) else {
        panic!("{:?}", parse_line(line));
    };
    assert_eq!(approval_id, "r1".into());
    assert_eq!(input["command"], "echo \u{fffd}");
}

#[test]
fn a_request_says_why_its_made_unless_what_it_says_is_blank() {
    let request = |reason: &str| {
        let line = format!(
            r#"{{"type":"control_request","request_id":"r1","request":{{"subtype":"can_use_tool","tool_name":"Bash","input":{{}},"decision_reason":"{reason}"}}}}"#
        );
        let Parsed::Event(EventBody::ApprovalRequested { reason, .. }) = parse_line(&line) else {
            panic!("{:?}", parse_line(&line));
        };
        reason
    };

    assert_eq!(request("It can't be checked").as_deref(), Some("It can't be checked"));
    assert_eq!(request("  "), None);
}

#[test]
fn surrogate_repair_only_touches_what_is_broken() {
    let text = |line: &str| {
        let parsed = parse_line(line);
        let Parsed::Event(EventBody::Assistant { blocks, .. }) = parsed else {
            panic!("{parsed:?}");
        };
        blocks
    };
    let wrap =
        |inner: &str| format!(r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{inner}"}}]}}}}"#);
    let block = |s: &str| vec![ContentBlock::Text { text: s.to_string(), images: Vec::new() }];
    // A whole pair is an ordinary character.
    assert_eq!(text(&wrap(r"\ud83d\ude00 ok")), block("\u{1f600} ok"));
    // A low surrogate with no high one, and a high one followed by something else.
    assert_eq!(text(&wrap(r"a\ude00b")), block("a\u{fffd}b"));
    assert_eq!(text(&wrap(r"a\ud83d\u0041b")), block("a\u{fffd}Ab"));
    assert_eq!(text(&wrap(r"\ud83d\ud83d\ude00")), block("\u{fffd}\u{1f600}"));
    // An escaped backslash means this is literal text, not an escape.
    assert_eq!(text(&wrap(r"\\ud83d \ud83d")), block("\\ud83d \u{fffd}"));
}

#[test]
fn unknown_types_and_subtypes_are_kept() {
    assert_eq!(
        parse_line(r#"{"type":"brand_new"}"#),
        Parsed::Event(EventBody::AgentOther { kind: "brand_new".into(), parent_tool_use_id: None })
    );
    assert_eq!(
        parse_line(r#"{"type":"system","subtype":"brand_new"}"#),
        Parsed::Event(EventBody::AgentOther { kind: "system/brand_new".into(), parent_tool_use_id: None })
    );
}

#[test]
fn an_interrupted_turn_reads_as_cancelled_and_other_errors_as_errors() {
    let cancelled = json!({"type":"result","is_error":true,"terminal_reason":"aborted_tools"}).to_string();
    let Parsed::Event(EventBody::TurnEnded { outcome, .. }) = parse_line(&cancelled) else { panic!() };
    assert_eq!(outcome, TurnOutcome::Cancelled);
    let failed = json!({"type":"result","is_error":true,"errors":["boom"]}).to_string();
    let Parsed::Event(EventBody::TurnEnded { outcome, errors, .. }) = parse_line(&failed) else { panic!() };
    assert_eq!((outcome, errors), (TurnOutcome::Error, vec!["boom".to_string()]));
}

#[test]
fn control_requests_other_than_approval_ask_for_an_error_reply() {
    let line = json!({"type":"control_request","request_id":"r1","request":{"subtype":"hook_callback"}}).to_string();
    assert_eq!(
        parse_line(&line),
        Parsed::UnsupportedControl { request_id: "r1".into(), subtype: "hook_callback".into() }
    );
}

/// The start of a PNG, which is enough for its bytes to say what it is.
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

/// A read of an image, as the CLI sends it: in the result, and again in its own record of the read.
fn read_an_image(base64: &str) -> String {
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

    let [ContentBlock::ToolResult { text, images: named, .. }] = blocks.as_slice() else { panic!("{blocks:?}") };
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
        assert!(matches!(blocks.as_slice(), [ContentBlock::ToolResult { images, .. }] if images.is_empty()));
    }
}

#[test]
fn a_line_written_for_the_model_isn_t_shown() {
    let note =
        "[Image: original 2640x1720, displayed at 2000x1303. Multiply coordinates by 1.32 to map to original image.]";
    let line =
        json!({"type":"user","message":{"role":"user","content":[{"type":"text","text":note}]},"isSynthetic":true});

    assert_eq!(
        parse_line(&line.to_string()),
        Parsed::Event(EventBody::AgentOther { kind: "user/synthetic".into(), parent_tool_use_id: None })
    );
}

#[test]
fn a_result_written_for_the_model_is_still_its_call_s_result() {
    let line = json!({"type":"user","message":{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"t1","content":"Interrupted","is_error":true
    }]},"isSynthetic":true});

    let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line.to_string()) else { panic!() };

    assert!(matches!(blocks.as_slice(), [ContentBlock::ToolResult { is_error: true, .. }]), "{blocks:?}");
}

#[test]
fn an_edit_s_result_carries_what_it_changed() {
    let line = json!({"type":"user","message":{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"t1","content":"The file has been updated."
    }]},"tool_use_result":{"filePath":"/w/a.rs","structuredPatch":[
        {"oldStart":3,"oldLines":1,"newStart":3,"newLines":1,"lines":["-old","+new"]}
    ]}})
    .to_string();

    let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line) else { panic!() };

    let [ContentBlock::ToolResult { change: Some(FileChange::Changed { hunks }), .. }] = blocks.as_slice() else {
        panic!("{blocks:?}")
    };
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
        let [ContentBlock::ToolResult { text, .. }] = blocks.as_slice() else { panic!("{blocks:?}") };
        text.clone()
    };

    assert_eq!(result("(Bash completed with no output)", ""), "");
    assert_eq!(result("Command running in background with ID: b1", ""), "Command running in background with ID: b1");
    assert_eq!(result("(Bash completed with no output)", "x"), "(Bash completed with no output)", "it printed");
}

#[test]
fn a_mode_change_is_reported_as_a_mode_a_chat_can_have_or_as_unknown() {
    let line = r#"{"type":"system","subtype":"status","status":null,"permissionMode":"plan"}"#;
    assert_eq!(parse_line(line), Parsed::Event(EventBody::AgentMode { permission_mode: PermissionMode::Plan }));
    let line = r#"{"type":"system","subtype":"status","status":null,"permissionMode":"default"}"#;
    assert_eq!(parse_line(line), Parsed::Event(EventBody::AgentMode { permission_mode: PermissionMode::Unknown }));
}

#[test]
fn a_status_about_something_else_isnt_a_mode_change() {
    let line = r#"{"type":"system","subtype":"status","status":"compacting"}"#;
    assert_eq!(
        parse_line(line),
        Parsed::Event(EventBody::AgentOther { kind: "system/status".into(), parent_tool_use_id: None })
    );
}

#[test]
fn the_clis_answers_to_control_requests_are_matched_up() {
    let line =
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"r","response":{"mode":"plan"}}}"#;
    assert_eq!(parse_line(line), Parsed::ControlReply { request_id: "r".into(), answer: Ok(json!({"mode": "plan"})) });
    let line = r#"{"type":"control_response","response":{"subtype":"success","request_id":"r"}}"#;
    assert_eq!(parse_line(line), Parsed::ControlReply { request_id: "r".into(), answer: Ok(Value::Null) });
    let line =
        r#"{"type":"control_response","response":{"subtype":"error","request_id":"r","error":"Model 'x' not found"}}"#;
    assert_eq!(
        parse_line(line),
        Parsed::ControlReply { request_id: "r".into(), answer: Err("Model 'x' not found".into()) }
    );
}

#[test]
fn a_finished_block_takes_its_place_from_its_own_deltas_only() {
    let delta = |index: u32| {
        json!({"type":"stream_event","api_message_id":"m","event":{"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":"x"}}})
            .to_string()
    };
    let finished = |texts: &[&str]| {
        let content: Vec<Value> = texts.iter().map(|text| json!({"type":"text","text":text})).collect();
        json!({"type":"assistant","message":{"id":"m","content":content}}).to_string()
    };
    let first_index = |parsed: Parsed| {
        let Parsed::Event(EventBody::Assistant { first_index, .. }) = &parsed else { panic!("{parsed:?}") };
        *first_index
    };
    let ended = json!({"type":"result","subtype":"success","is_error":false}).to_string();
    let mut normalizer = Normalizer::default();

    normalizer.line(&delta(2));
    assert_eq!(first_index(normalizer.line(&finished(&["x"])).parsed), Some(2));
    assert_eq!(first_index(normalizer.line(&finished(&["y"])).parsed), None, "a block that didn't stream");
    normalizer.line(&delta(3));
    assert_eq!(first_index(normalizer.line(&finished(&["x", "y"])).parsed), None, "which of them streamed is unknown");
    normalizer.line(&delta(4));
    normalizer.line(&ended);
    assert_eq!(
        first_index(normalizer.line(&finished(&["x"])).parsed),
        None,
        "a turn that ended left nothing streaming"
    );
}

#[test]
fn a_prompts_lifecycle_names_it_by_the_uuid_it_was_sent_with() {
    let line = |state: &str| {
        json!({"type":"command_lifecycle","command_uuid":"u1","state":state,"uuid":"other","session_id":"s"})
            .to_string()
    };
    let progress = |state: &str| {
        let Parsed::Prompt { uuid, progress } = parse_line(&line(state)) else { panic!("{state}") };
        assert_eq!(uuid, "u1");
        progress
    };

    assert_eq!(progress("started"), PromptProgress::Started);
    assert_eq!(progress("cancelled"), PromptProgress::Cancelled);
    assert_eq!(progress("queued"), PromptProgress::Other);
    assert_eq!(progress("completed"), PromptProgress::Other);
    assert_eq!(progress("something newer"), PromptProgress::Other);
    let nameless = json!({"type":"command_lifecycle","state":"started"}).to_string();
    assert_eq!(
        parse_line(&nameless),
        Parsed::Event(EventBody::AgentOther { kind: "command_lifecycle".into(), parent_tool_use_id: None })
    );
}

/// A line from the agent with only `block`, as Claude sends each block of a message.
fn from_the_agent(block: &Value) -> String {
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
    assert_eq!(
        blocks,
        [fixtures::tool_use("srvtoolu_1", "advisor", json!({}))],
        "as a line alone reads, before the normalizer says what kind of call it is"
    );
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
        let [ContentBlock::ToolResult { tool_use_id, is_error, text, encrypted, .. }] = blocks.as_slice() else {
            panic!("{blocks:?}")
        };
        assert_eq!(tool_use_id.as_str(), "srvtoolu_1");
        (*is_error, text.clone(), *encrypted)
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

#[test]
fn only_a_line_from_a_subagent_names_it() {
    let note = json!({"type":"user","message":{"content":[]},"isSynthetic":true,"parent_tool_use_id":"agent"});
    // About a call, the agent's own or one starting a subagent, which `parent_tool_use_id` names.
    let progress = json!({"type":"tool_progress","tool_use_id":"t1","tool_name":"Bash","parent_tool_use_id":"bash"});
    // About a subagent, but the agent's own: it names the call in `tool_use_id`.
    let started = json!({"type":"system","subtype":"task_started","tool_use_id":"agent","parent_tool_use_id":null});

    let subagent = |line: serde_json::Value| {
        let Parsed::Event(body @ EventBody::AgentOther { .. }) = parse_line(&line.to_string()) else {
            panic!("{line}")
        };
        body.subagent().map(ToolUseId::to_string)
    };

    assert_eq!(subagent(note).as_deref(), Some("agent"));
    assert_eq!(subagent(progress), None);
    assert_eq!(subagent(started), None);
}

#[test]
fn a_reply_names_the_model_it_came_from() {
    let model = |model: &str| {
        let line =
            json!({"type":"assistant","parent_tool_use_id":"agent","message":{"id":"m","model":model,"content":[]}});
        let parsed = parse_line(&line.to_string());
        let Parsed::Event(EventBody::Assistant { model, .. }) = parsed else { panic!("{parsed:?}") };
        model
    };

    assert_eq!(model("claude-haiku-5-5").as_deref(), Some("claude-haiku-5-5"));
}
