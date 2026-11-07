use serde_json::json;

use super::*;

#[test]
fn output_that_is_not_json_is_kept_rather_than_failing() {
    assert_eq!(parse_line("Warning: something"), Parsed::Event(EventBody::AgentOther { kind: "unparsed".into() }));
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
    let block = |s: &str| vec![ContentBlock::Text { text: s.to_string() }];
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
        Parsed::Event(EventBody::AgentOther { kind: "brand_new".into() })
    );
    assert_eq!(
        parse_line(r#"{"type":"system","subtype":"brand_new"}"#),
        Parsed::Event(EventBody::AgentOther { kind: "system/brand_new".into() })
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

#[test]
fn tool_results_flatten_text_and_count_images() {
    let line = json!({"type":"user","message":{"role":"user","content":[{
        "type":"tool_result","tool_use_id":"t1","content":[
            {"type":"text","text":"a"},
            {"type":"image","source":{"data":"…"}},
            {"type":"text","text":"b"}
        ]
    }]}})
    .to_string();
    let Parsed::Event(EventBody::AgentUser { blocks, .. }) = parse_line(&line) else { panic!() };
    assert_eq!(
        blocks,
        vec![ContentBlock::ToolResult {
            tool_use_id: "t1".into(),
            is_error: false,
            text: "a\nb".into(),
            image_count: 1
        }]
    );
}

#[test]
fn a_mode_change_is_reported_by_the_name_a_launch_accepts() {
    let line = r#"{"type":"system","subtype":"status","status":null,"permissionMode":"plan"}"#;
    assert_eq!(parse_line(line), Parsed::Event(EventBody::AgentMode { permission_mode: "plan".into() }));
    let line = r#"{"type":"system","subtype":"status","status":null,"permissionMode":"default"}"#;
    assert_eq!(parse_line(line), Parsed::Event(EventBody::AgentMode { permission_mode: "manual".into() }));
}

#[test]
fn a_status_about_something_else_isnt_a_mode_change() {
    let line = r#"{"type":"system","subtype":"status","status":"compacting"}"#;
    assert_eq!(parse_line(line), Parsed::Event(EventBody::AgentOther { kind: "system/status".into() }));
}

#[test]
fn the_clis_answers_to_control_requests_are_matched_up() {
    let line =
        r#"{"type":"control_response","response":{"subtype":"success","request_id":"r","response":{"mode":"plan"}}}"#;
    assert_eq!(parse_line(line), Parsed::ControlReply { request_id: "r".into(), error: None });
    let line =
        r#"{"type":"control_response","response":{"subtype":"error","request_id":"r","error":"Model 'x' not found"}}"#;
    assert_eq!(
        parse_line(line),
        Parsed::ControlReply { request_id: "r".into(), error: Some("Model 'x' not found".into()) }
    );
}
