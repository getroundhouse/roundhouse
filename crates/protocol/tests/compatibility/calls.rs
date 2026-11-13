//! What has to stay readable of a tool call and its result, which the log holds for good.

use protocol::event::{Asking, CallKind, ContentBlock, EventBody};
use protocol::model::Call;

/// Spelled out, since the log holds this form for good. A call's ID is a plain string, as its result's is.
#[test]
fn a_call_and_its_result_keep_their_form() {
    let call: &[u8] = br#"{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"}}"#;
    let result: &[u8] = br#"{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"text":"a.rs"}"#;
    let read = |block: &[u8]| -> ContentBlock { serde_json::from_slice(block).unwrap() };

    let (ContentBlock::ToolUse { id, .. }, ContentBlock::ToolResult { tool_use_id, .. }) = (read(call), read(result))
    else {
        panic!("a call and a result read as themselves");
    };

    assert_eq!(id, tool_use_id, "a result names its call by the call's own ID");
    for block in [call, result] {
        assert_eq!(serde_json::to_vec(&read(block)).unwrap(), block, "it's written the way it's read");
    }
}

/// The same, from an adapter that says what kind of call each is. A build from before kinds reads these as it reads
/// the ones above, since it ignores what it doesn't know.
#[test]
fn a_call_and_its_result_that_say_their_kind_keep_their_form() {
    let call: &[u8] =
        br#"{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls x"},"call":{"kind":"command"}}"#;
    let result: &[u8] = br#"{"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"text":"ls: x: No such file","call":{"kind":"command"},"exit_code":2}"#;
    let from_a_server: &[u8] = br#"{"type":"tool_use","id":"toolu_2","name":"mcp__github__list","input":{},"call":{"kind":"other","server":"github","tool":"list"}}"#;
    let read = |block: &[u8]| -> ContentBlock { serde_json::from_slice(block).unwrap() };

    let ContentBlock::ToolUse { call: said, .. } = read(call) else { panic!("a call reads as one") };
    assert_eq!(said, Some(CallKind::Command));
    let ContentBlock::ToolResult { call: said, exit_code, text, .. } = read(result) else { panic!("a result") };
    assert_eq!((said, exit_code, text.as_str()), (Some(CallKind::Command), Some(2), "ls: x: No such file"));
    let ContentBlock::ToolUse { call: said, .. } = read(from_a_server) else { panic!("a call reads as one") };
    assert_eq!(said, Some(CallKind::Other { server: Some("github".into()), tool: "list".into() }));
    for block in [call, result, from_a_server] {
        assert_eq!(serde_json::to_vec(&read(block)).unwrap(), block, "it's written the way it's read");
    }
}

/// A kind a newer adapter says, which this build doesn't know, costs only the kind: the call still reads, by its tool.
#[test]
fn a_call_of_a_kind_this_build_doesnt_know_still_reads() {
    let call =
        br#"{"type":"tool_use","id":"toolu_1","name":"NotebookEdit","input":{},"call":{"kind":"notebook","cell":3}}"#;

    let ContentBlock::ToolUse { name, call, .. } = serde_json::from_slice(call).unwrap() else { panic!("a call") };

    assert_eq!((name.as_str(), call), ("NotebookEdit", Some(CallKind::Unknown)));
}

#[test]
fn a_request_for_approval_and_a_subagent_s_call_say_the_kind_the_same_way() {
    let request = br#"{"type":"approval_requested","approval_id":"a","tool_name":"Bash","tool_use_id":"toolu_1","input":{"command":"ls"},"description":null,"suggestions":null,"asking":{"kind":"tool_use","call":{"kind":"command"}}}"#;
    let body: EventBody = serde_json::from_slice(request).unwrap();
    let EventBody::ApprovalRequested { asking, .. } = &body else { panic!("a request reads as one") };
    assert_eq!(asking.as_deref().and_then(Asking::call), Some(&CallKind::Command));
    assert_eq!(serde_json::to_vec(&body).unwrap(), request);
    // One recorded before adapters said the kind, which is written back as it was.
    let before = Asking::ToolUse { call: None };
    assert_eq!(serde_json::to_string(&before).unwrap(), r#"{"kind":"tool_use"}"#);
    assert_eq!(serde_json::from_str::<Asking>(r#"{"kind":"tool_use"}"#).unwrap(), before);

    let made = br#"{"id":"toolu_1","name":"Grep","input":{"pattern":"x"},"kind":{"kind":"search"}}"#;
    let call: Call = serde_json::from_slice(made).unwrap();
    assert_eq!(call.kind, Some(CallKind::Search));
    assert_eq!(serde_json::to_vec(&call).unwrap(), made);
    let older = br#"{"id":"toolu_1","name":"Grep","input":{"pattern":"x"}}"#;
    let call: Call = serde_json::from_slice(older).unwrap();
    assert_eq!(call.kind, None);
    assert_eq!(serde_json::to_vec(&call).unwrap(), older);
}

/// A build from before kinds has a tool's use as a variant with no fields. It has to go on reading one that says its
/// kind, as what it knows: a tool's use.
#[test]
fn a_build_from_before_kinds_reads_a_request_that_says_its_kind() {
    #[derive(serde::Deserialize, Debug, PartialEq)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum AskingBefore {
        ToolUse,
        #[serde(other)]
        Unknown,
    }

    let newer = r#"{"kind":"tool_use","call":{"kind":"command"}}"#;

    assert_eq!(serde_json::from_str::<AskingBefore>(newer).unwrap(), AskingBefore::ToolUse);
}
