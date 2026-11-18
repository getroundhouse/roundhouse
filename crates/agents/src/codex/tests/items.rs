//! Codex's waits, web searches and compactions, as its adapter records them, and what it leaves out.

use protocol::capability::calls::call::{Fetch, FindInPage, Sleep, WebSearch};
use protocol::capability::calls::output::WebResult;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{CompactedBy, CompactionOutcome, ContentBlock, EventBody};
use serde_json::{Value, json};

use super::script::{bodies, item, others, play, turn};
use crate::AgentOutput;

/// What the adapter recorded, with the raw line it was recorded with.
type Recorded<'a, T> = Vec<(&'a T, &'a str)>;

/// Each call among `output`, and each result, with the raw line each was recorded with.
fn calls(output: &[AgentOutput]) -> (Recorded<'_, Call>, Recorded<'_, CallOutput>) {
    let (mut calls, mut results) = (Vec::new(), Vec::new());
    for out in output {
        let AgentOutput::Event { body, raw: Some(raw), .. } = out else { continue };
        if let EventBody::Assistant { blocks, .. } = body {
            calls.extend(blocks.iter().filter_map(|block| {
                let ContentBlock::Call { call, .. } = block else { return None };
                Some((call, raw.as_str()))
            }));
        }
        if let EventBody::AgentUser { blocks, .. } = body {
            results.extend(blocks.iter().filter_map(|block| {
                let ContentBlock::CallResult { output, .. } = block else { return None };
                Some((output, raw.as_str()))
            }));
        }
    }
    (calls, results)
}

#[tokio::test]
async fn a_wait_is_a_call_to_wait_for_as_long_as_it_says() {
    let sleep = json!({"type":"sleep","id":"sleep-1","durationMs":20_000,"startedAtMs":1,"completedAtMs":null});

    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        item("thread", "root-turn", sleep.clone(), false),
        item("thread", "root-turn", sleep, true),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    let (calls, results) = calls(&output);
    assert_eq!(calls.iter().map(|(call, _)| *call).collect::<Vec<_>>(), [&Call::Sleep(Sleep { duration_ms: 20_000 })]);
    assert_eq!(results.len(), 1, "it ends once it has waited");
    assert_eq!(others(&output), Vec::<&str>::new());
}

/// A search's start says nothing of what it searches for, so its call is recorded as it completes, with the line that
/// started it, and what came back with the line that completed it.
#[tokio::test]
async fn a_web_search_is_recorded_as_what_it_did_once_its_done() {
    let started = |id: &str| json!({"type":"webSearch","id":id,"query":"","action":null,"results":null});
    let found = json!([{"type":"text_result","domain":"example.com","ref_id":"turn0search0",
        "title":"Release notes","url":"https://example.com/notes","snippet":"What changed."}]);
    let completed = |id: &str, action: Value| json!({"type":"webSearch","id":id,"query":"release notes","action":action,"results":found});
    let searches = [
        ("search-1", json!({"type":"search","query":"release notes","queries":null})),
        ("open-1", json!({"type":"openPage","url":"https://example.com/notes"})),
        ("find-1", json!({"type":"findInPage","url":null,"pattern":"breaking"})),
        ("other-1", json!({"type":"other"})),
    ];
    let mut actions = vec![turn("thread", "root-turn", "inProgress")];
    for (id, action) in &searches {
        actions.push(item("thread", "root-turn", started(id), false));
        actions.push(item("thread", "root-turn", completed(id, action.clone()), true));
    }
    actions.push(turn("thread", "root-turn", "completed"));

    let output = play(actions).await;

    let (calls, results) = calls(&output);
    let made: Vec<_> = calls.iter().map(|(call, _)| (*call).clone()).collect();
    assert_eq!(
        made,
        [
            Call::WebSearch(WebSearch {
                query: "release notes".into(),
                only_domains: Vec::new(),
                not_domains: Vec::new()
            }),
            Call::Fetch(Fetch { url: "https://example.com/notes".into(), asking: None }),
            Call::FindInPage(FindInPage { url: String::new(), pattern: "breaking".into() }),
            Call::Other { tool: "webSearch".into(), input: json!({"query":"release notes"}) },
        ]
    );
    for ((_, call_raw), (_, result_raw)) in calls.iter().zip(&results) {
        assert!(call_raw.contains("item/started"), "the call keeps the line that started it: {call_raw}");
        assert!(result_raw.contains("item/completed"), "{result_raw}");
    }
    let result = CallOutput::WebResults {
        results: vec![WebResult {
            title: "Release notes".into(),
            url: "https://example.com/notes".into(),
            snippet: Some("What changed.".into()),
        }],
        summary: None,
    };
    assert!(results.iter().all(|(output, _)| **output == result), "{results:?}");
    assert_eq!(others(&output), Vec::<&str>::new());
}

#[tokio::test]
async fn codex_compacting_its_context_is_a_compaction_it_started() {
    let compaction = |completed: Option<u64>| json!({"type":"contextCompaction","id":"compact-1","startedAtMs":1_000,"completedAtMs":completed});

    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        item("thread", "root-turn", compaction(None), false),
        item("thread", "root-turn", compaction(Some(120_000)), true),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    let compactions: Vec<_> = bodies(&output)
        .into_iter()
        .filter(|body| matches!(body, EventBody::AgentCompacting { .. } | EventBody::AgentCompacted { .. }))
        .collect();
    let outcome = CompactionOutcome::Compacted {
        by: CompactedBy::Agent,
        before_tokens: None,
        after_tokens: None,
        took_ms: Some(119_000),
        summary: None,
    };
    assert_eq!(
        compactions,
        [
            &EventBody::AgentCompacting { written_chars: None },
            &EventBody::AgentCompacted { asked: None, keeping: None, outcome }
        ]
    );
    assert_eq!(others(&output), Vec::<&str>::new());
}

/// Codex waiting on a running command with nothing to write leaves nothing, and neither do the thread's settings as it
/// starts, which the adapter sets with each turn. What's written to a command is kept for a later build.
#[tokio::test]
async fn a_wait_on_a_command_and_the_threads_settings_leave_nothing() {
    let interaction = |stdin: &str| {
        json!({"method":"item/commandExecution/terminalInteraction","params":{"threadId":"thread","turnId":"root-turn",
            "itemId":"call-1","processId":"1","stdin":stdin}})
    };
    let settings = json!({"method":"thread/settings/updated","params":{"threadId":"thread",
        "threadSettings":{"model":"gpt-6","effort":"low"}}});

    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        settings,
        interaction(""),
        interaction("y\n"),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    assert_eq!(others(&output), ["item/commandExecution/terminalInteraction"]);
}
