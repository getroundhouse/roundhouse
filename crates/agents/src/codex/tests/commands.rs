//! What a Codex command does, as its adapter reads it.

use protocol::capability::calls::Call;
use protocol::capability::calls::call::CommandPart;
use protocol::event::{ContentBlock, EventBody};
use serde_json::{Value, json};

use super::script::{bodies, item, play, turn};

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
