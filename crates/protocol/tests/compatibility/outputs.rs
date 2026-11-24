//! What a typed call sent back, in the forms a build writes, and a subagent's record of its typed calls.

use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::{Found, McpContent, Paths};
use protocol::event::ContentBlock;
use protocol::model::Subagent;
use serde_json::{Value, json};

fn reads_back_as_written(wire: &Value) -> ContentBlock {
    let block: ContentBlock = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(&serde_json::to_value(&block).unwrap(), wire);
    block
}

fn output(block: ContentBlock) -> CallOutput {
    let ContentBlock::CallResult { output, .. } = block else { panic!("{block:?}") };
    output
}

#[test]
fn a_typed_result_keeps_its_images_beside_what_the_call_did() {
    let block = reads_back_as_written(&json!({"type":"call_result","tool_use_id":"c","is_error":false,
        "output":{"kind":"command","output":"shot","ended":"exited","exit_code":0},
        "images":[{"id":"i","name":"shot.png","media_type":"image/png","size_bytes":3}]}));

    let ContentBlock::CallResult { images, .. } = block else { panic!() };
    assert_eq!(images.len(), 1);
    let without = reads_back_as_written(&json!({"type":"call_result","tool_use_id":"c","is_error":false,
        "output":{"kind":"words","text":"ok"}}));
    let ContentBlock::CallResult { images, .. } = without else { panic!() };
    assert!(images.is_empty(), "one from before images were kept has none");
}

#[test]
fn an_encrypted_or_backgrounded_result_and_a_searchs_summary_keep_their_forms() {
    let result = |output: Value| json!({"type":"call_result","tool_use_id":"c","is_error":false,"output":output});

    assert_eq!(output(reads_back_as_written(&result(json!({"kind":"encrypted"})))), CallOutput::Encrypted);
    let later = output(reads_back_as_written(&result(json!({"kind":"backgrounded","task":"t"}))));
    assert_eq!(later, CallOutput::Backgrounded { task: Some("t".into()) });
    let unnamed = output(reads_back_as_written(&result(json!({"kind":"backgrounded"}))));
    assert_eq!(unnamed, CallOutput::Backgrounded { task: None });
    let found = output(reads_back_as_written(&result(json!({"kind":"web_results","results":[],"summary":"two"}))));
    assert_eq!(found, CallOutput::WebResults { results: Vec::new(), summary: Some("two".into()) });
    let before = output(reads_back_as_written(&result(json!({"kind":"web_results","results":[]}))));
    assert_eq!(before, CallOutput::WebResults { results: Vec::new(), summary: None }, "one from before summaries");
}

/// A search for files and a search for the files that hold a match share one shape, in the forms each had before.
#[test]
fn found_paths_keep_the_form_they_had() {
    let result = |output: Value| json!({"type":"call_result","tool_use_id":"c","is_error":false,"output":output});
    let paths = Paths { paths: vec!["a.rs".into()], truncated: true };

    let listed = output(reads_back_as_written(&result(json!({"kind":"paths","paths":["a.rs"],"truncated":true}))));
    assert_eq!(listed, CallOutput::Paths(paths.clone()));
    let wire = result(json!({"kind":"found","found":"files","paths":["a.rs"],"truncated":true}));
    assert_eq!(output(reads_back_as_written(&wire)), CallOutput::Found(Found::Files(paths)));
    let whole = Paths { paths: vec!["a.rs".into()], truncated: false };
    let all = output(reads_back_as_written(&result(json!({"kind":"paths","paths":["a.rs"]}))));
    assert_eq!(all, CallOutput::Paths(whole.clone()), "a list that wasn't cut short says nothing of it");
    let wire = result(json!({"kind":"found","found":"files","paths":["a.rs"]}));
    assert_eq!(output(reads_back_as_written(&wire)), CallOutput::Found(Found::Files(whole)));
}

#[test]
fn an_mcp_block_the_adapter_didnt_know_is_kept_and_one_this_build_doesnt_know_reads_as_unknown() {
    let result = |content: Value| {
        let output = json!({"kind":"mcp","content":content});
        json!({"type":"call_result","tool_use_id":"c","is_error":false,"output":output})
    };

    let kept = output(reads_back_as_written(&result(json!([{"type":"other","value":{"type":"audio"}}]))));
    assert_eq!(
        kept,
        CallOutput::Mcp { content: vec![McpContent::Other { value: json!({"type":"audio"}) }], structured: None }
    );
    let future: ContentBlock = serde_json::from_value(result(json!([{"type":"future_block"}]))).unwrap();
    assert_eq!(output(future), CallOutput::Mcp { content: vec![McpContent::Unknown], structured: None });
    let unknown: ContentBlock = serde_json::from_value(json!({"type":"call_result","tool_use_id":"c",
        "is_error":false,"output":{"kind":"future_output"}}))
    .unwrap();
    assert_eq!(output(unknown), CallOutput::Unknown);
}

#[test]
fn a_subagents_record_keeps_its_typed_calls_and_one_from_before_has_none() {
    let record = json!({"session_id":"s","id":"c","call":"c","status":"working","started_at_ms":1,"calls":1,"rev":1,
        "lately":{"calls":[{"before":{"text":"Looking","cut":false},"id":"r",
            "call":{"kind":"read","path":"a.rs"},
            "result":{"output":{"kind":"words","text":"fn main"},"is_error":false,"cut":true}}],
        "since":{"text":"Done","cut":false}}});

    let read: Subagent = serde_json::from_value(record.clone()).unwrap();
    let Some(lately) = &read.lately else { panic!("{read:?}") };
    let [call] = lately.calls.as_slice() else { panic!("{read:?}") };
    assert_eq!(call.id.as_str(), "r");
    assert!(call.result.as_ref().is_some_and(|result| result.cut));
    let written = serde_json::to_value(&read).unwrap();
    assert_eq!(written["lately"], record["lately"], "as it was written");
    let older: Subagent =
        serde_json::from_value(json!({"session_id":"s","id":"c","call":"c","status":"working","rev":1,
        "started_at_ms":1,"calls":1,"latest_call":{"id":"r","name":"Read","input":{"file_path":"a.rs"}}}))
        .unwrap();
    assert_eq!(older.lately, None);
}
