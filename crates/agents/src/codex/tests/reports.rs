//! What Codex says of the agent's turn that isn't an event, as its adapter reports it, and what it asks Codex not to
//! send.

use protocol::capability::Report;
use protocol::capability::output::{Todo, TodoList, TodoStatus};
use serde_json::{Value, json};

use super::harness::Harness;
use super::script::{others, play, turn};
use crate::AgentOutput;
use crate::codex::notifications::UNUSED;

/// The reports among `output`.
fn reports(output: &[AgentOutput]) -> Vec<&Report> {
    output.iter().filter_map(|out| if let AgentOutput::Report(report) = out { Some(report) } else { None }).collect()
}

fn diff_updated(thread: &str, diff: &str) -> Value {
    json!({"method":"turn/diff/updated","params":{"threadId":thread,"turnId":"root-turn","diff":diff}})
}

/// Codex says the turn's diff again with every change, often the same, which is reported once.
#[tokio::test]
async fn a_turns_changes_are_reported_whole_once_each_time_they_change() {
    let made = "diff --git a/hello.txt b/hello.txt\nnew file mode 100644\n--- /dev/null\n+++ b/hello.txt\n@@ -0,0 +1 @@\n+hello\n";
    let more = format!(
        "{made}diff --git a/bye.txt b/bye.txt\nnew file mode 100644\n--- /dev/null\n+++ b/bye.txt\n@@ -0,0 +1 @@\n+bye\n"
    );

    let output = play(vec![
        turn("thread", "root-turn", "inProgress"),
        diff_updated("thread", made),
        diff_updated("thread", made),
        diff_updated("thread", &more),
        turn("thread", "root-turn", "completed"),
    ])
    .await;

    let changed: Vec<Vec<&str>> = reports(&output)
        .into_iter()
        .map(|report| {
            let Report::TurnChanges { files } = report else { panic!("{report:?}") };
            files.iter().map(|file| file.path.as_str()).collect()
        })
        .collect();
    assert_eq!(changed, [vec!["hello.txt"], vec!["hello.txt", "bye.txt"]]);
}

#[tokio::test]
async fn codexs_plan_is_reported_as_its_to_do_list() {
    let plan = json!({"method":"turn/plan/updated","params":{"threadId":"thread","turnId":"root-turn",
        "explanation":"Two steps","plan":[{"step":"Write hello","status":"completed"},
        {"step":"Say done","status":"inProgress"},{"step":"Rest","status":"pending"}]}});

    let output =
        play(vec![turn("thread", "root-turn", "inProgress"), plan, turn("thread", "root-turn", "completed")]).await;

    let todo =
        |text: &str, status| Todo { id: None, text: text.into(), status, doing: None, priority: None, after: vec![] };
    let items = vec![
        todo("Write hello", TodoStatus::Done),
        todo("Say done", TodoStatus::InProgress),
        todo("Rest", TodoStatus::Pending),
    ];
    assert_eq!(reports(&output), [&Report::Todos(TodoList { items, note: Some("Two steps".into()) })]);
}

/// So none of them is read, or kept as a line the adapter doesn't understand, and a Codex that sends one anyway has it
/// dropped.
#[tokio::test]
async fn codex_is_asked_not_to_send_what_the_adapter_doesnt_use_and_what_it_sends_anyway_is_dropped() {
    let mut h = Harness::new(&json!([]));
    let initialized = h.request("initialize").await;
    h.started().await;
    assert_eq!(initialized["params"]["capabilities"]["optOutNotificationMethods"], json!(UNUSED));

    let anyway: Vec<Value> =
        UNUSED.iter().map(|method| json!({"method":method,"params":{"threadId":"thread"}})).collect();
    let mut actions = vec![turn("thread", "root-turn", "inProgress")];
    actions.extend(anyway);
    actions.push(turn("thread", "root-turn", "completed"));
    let output = play(actions).await;
    assert_eq!(others(&output), Vec::<&str>::new());
}
