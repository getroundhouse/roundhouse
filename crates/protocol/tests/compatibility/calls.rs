//! What has to stay readable of a call and its result, and of a request to make one.

use protocol::capability::calls::Call as Typed;
use protocol::event::{Asking, ContentBlock, EventBody};

/// The forms Claude's adapter recorded calls and results in before it typed them, which the hub's migration typed. A
/// hub from before the migration still sends them, and a client reads each as a block it doesn't know, rather than
/// failing the event it's in.
#[test]
fn a_call_or_its_result_in_an_older_form_reads_as_a_block_this_build_doesnt_know() {
    let older = [
        br#"{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"ls"},"call":{"kind":"command"}}"#
            .as_slice(),
        br#"{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"text":"a.rs","call":{"kind":"command"}}"#,
    ];

    for block in older {
        assert_eq!(serde_json::from_slice::<ContentBlock>(block).unwrap(), ContentBlock::Unknown);
    }
}

#[test]
fn a_request_for_approval_says_the_call_it_asks_to_make() {
    let request = br#"{"type":"approval_requested","approval_id":"a","tool_name":"Bash","tool_use_id":"toolu_1","input":{"command":"ls"},"description":null,"suggestions":null,"asking":{"kind":"tool_use","typed":{"kind":"command","command":"ls","runs":"waits"}}}"#;

    let body: EventBody = serde_json::from_slice(request).unwrap();

    let EventBody::ApprovalRequested { asking, .. } = &body else { panic!("a request reads as one") };
    let Some(Asking::ToolUse { typed: Some(Typed::Command(command)) }) = asking.as_deref() else {
        panic!("{asking:?}")
    };
    assert_eq!(command.command, "ls");
    assert_eq!(serde_json::to_vec(&body).unwrap(), request);
}

/// One recorded when requests said only their call's kind still reads, as one whose call this build can't read, rather
/// than failing as a whole. The hub's migration turns each into its call.
#[test]
fn a_request_that_says_only_its_call_s_kind_reads_as_one_whose_call_isnt_known() {
    let request = br#"{"kind":"tool_use","call":{"kind":"command"}}"#;

    assert_eq!(serde_json::from_slice::<Asking>(request).unwrap(), Asking::ToolUse { typed: None });
}

/// One recorded before adapters said the kind, which is written back as it was.
#[test]
fn a_request_from_before_kinds_keeps_its_form() {
    let before = Asking::ToolUse { typed: None };

    assert_eq!(serde_json::to_string(&before).unwrap(), r#"{"kind":"tool_use"}"#);
    assert_eq!(serde_json::from_str::<Asking>(r#"{"kind":"tool_use"}"#).unwrap(), before);
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
    let typed = r#"{"kind":"tool_use","typed":{"kind":"command","command":"ls"}}"#;

    assert_eq!(serde_json::from_str::<AskingBefore>(newer).unwrap(), AskingBefore::ToolUse);
    assert_eq!(serde_json::from_str::<AskingBefore>(typed).unwrap(), AskingBefore::ToolUse);
}
