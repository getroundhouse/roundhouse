//! What a Codex command does, as its adapter reads it.

use protocol::capability::calls::call::CommandPart;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{ContentBlock, EventBody};
use serde_json::{Value, json};

use super::script::{bodies, command, item, play, turn};

/// Codex's command `id`, run as Codex runs `script`, with what Codex read of it, `actions`, as it starts and ends.
fn ran(id: &str, script: &str, actions: &Value) -> [Value; 2] {
    let command = format!("/bin/zsh -c '{script}'");
    let body = |status: &str| {
        json!({"type":"commandExecution","id":id,"command":command,"cwd":"/w","commandActions":actions,
            "status":status,"aggregatedOutput":"","exitCode":0})
    };
    [item("thread", "root-turn", body("inProgress"), false), item("thread", "root-turn", body("completed"), true)]
}

/// A command says what it does by its own words, not by Codex's reading of it, which can leave out what else it does,
/// and says it of one Codex said nothing of.
#[tokio::test]
async fn a_command_says_what_it_does_by_its_words_rather_than_by_what_codex_read() {
    let read = json!([{"type":"read","command":"cat a.rs","name":"a.rs","path":"/w/a.rs"}]);
    let mut actions = vec![turn("thread", "root-turn", "inProgress")];
    actions.extend(ran("deletes", "cat a.rs | xargs rm", &read));
    actions.extend(ran("reads", "cat a.rs", &json!([])));
    actions.push(turn("thread", "root-turn", "completed"));
    let output = play(actions).await;

    let parts = |id: &str| {
        bodies(&output).into_iter().find_map(|body| {
            let EventBody::Assistant { blocks, .. } = body else { return None };
            blocks.iter().find_map(|block| {
                let ContentBlock::Call { id: call, call: Call::Command(command) } = block else { return None };
                (call.as_str() == id).then(|| command.parts.clone())
            })
        })
    };
    assert_eq!(parts("deletes"), Some(vec![CommandPart::Other]));
    assert_eq!(parts("reads"), Some(vec![CommandPart::Read { path: "a.rs".into() }]));
}

#[tokio::test]
async fn command_output_chunks_are_transient_but_the_completed_output_and_unknown_events_are_kept() {
    let mut actions = vec![turn("thread", "root-turn", "inProgress"), command("thread", "root-turn", "cmd", false)];
    for chunk in ["he", "ll", "o"] {
        actions.push(json!({"method":"item/commandExecution/outputDelta",
            "params":{"threadId":"thread","turnId":"root-turn","itemId":"cmd","delta":chunk}}));
    }
    actions.push(command("thread", "root-turn", "cmd", true));
    actions.push(json!({"method":"future/notification","params":{"threadId":"thread","value":"kept"}}));
    actions.push(turn("thread", "root-turn", "completed"));

    let output = play(actions).await;
    let bodies = bodies(&output);
    assert!(!bodies.iter().any(|body| matches!(body, EventBody::AgentOther { kind, .. }
        if kind == "item/commandExecution/outputDelta")));
    let results: Vec<_> = bodies
        .iter()
        .filter_map(|body| {
            let EventBody::AgentUser { blocks, .. } = body else { return None };
            blocks.iter().find_map(|block| {
                let ContentBlock::CallResult { tool_use_id, output: CallOutput::Command(command), .. } = block else {
                    return None;
                };
                (tool_use_id.as_str() == "cmd").then_some(command)
            })
        })
        .collect();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].output, "hello");
    assert_eq!(results[0].exit_code, Some(0));
    assert!(output.iter().any(|out| matches!(out, crate::AgentOutput::Event {
        body: EventBody::AgentOther { kind, .. }, raw: Some(raw), ..
    } if kind == "future/notification" && raw.contains("kept"))));
}
