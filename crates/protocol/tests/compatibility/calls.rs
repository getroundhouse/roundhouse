//! What has to stay readable of a call and its result.

use protocol::event::ContentBlock;

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
