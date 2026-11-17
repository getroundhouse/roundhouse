//! A subagent whose agent types its calls, as Codex's does: what its record counts and keeps of what it does lately.

use super::super::*;
use super::recording::{ended, exited, message, record, started, subagent};
use crate::store::tests::store_with_session;
use protocol::capability::calls::Call as Typed;
use protocol::capability::calls::CallOutput;
use protocol::capability::calls::output::Ended;
use protocol::event::TaskOutcome;
use protocol::fixtures::{command_output, typed_command};

fn said(text: &str) -> ContentBlock {
    ContentBlock::Text { text: text.into(), images: Vec::new() }
}

fn command(id: &str, command: &str) -> ContentBlock {
    ContentBlock::Call { id: id.into(), call: Typed::Command(typed_command(command)) }
}

/// What came back for the typed call `id` to the subagent `to` started.
fn printed(to: &str, id: &str, output: &str) -> EventBody {
    let output = command_output(output, Some(0), Ended::Exited);
    let result =
        ContentBlock::CallResult { tool_use_id: id.into(), output, is_error: false, denied: None, images: Vec::new() };
    EventBody::AgentUser { subagent: Some(to.into()), blocks: vec![result] }
}

/// A subagent's typed calls are counted, and what it's done lately is kept with each typed, with what came back cut
/// short, while it works, and let go of when it ends. Until it's made one, its record says what it wrote as the older
/// form does, so a client draws it the same.
#[test]
fn a_working_subagents_typed_calls_are_counted_and_kept_lately() {
    let dir = tempfile::tempdir().unwrap();
    let store = store_with_session(dir.path());
    record(&store, &started("codex"));
    record(&store, &started("other"));

    assert_eq!(record(&store, &message(Some("codex"), vec![said("I'll run the tests.")])).1, ["codex"]);
    let wrote = subagent(&store, "codex");
    assert_eq!(wrote.lately, None, "no typed call yet");
    assert_eq!(wrote.recent.since.map(|said| said.text).as_deref(), Some("I'll run the tests."));

    assert_eq!(record(&store, &message(Some("codex"), vec![command("test", "cargo test")])).1, ["codex"]);
    let long = "ok\n".repeat(2_000);
    assert_eq!(record(&store, &printed("codex", "test", &long)).1, ["codex"], "what came back");
    record(&store, &message(Some("codex"), vec![said("They pass.")]));
    record(&store, &message(Some("other"), vec![command("ls", "ls")]));

    let working = subagent(&store, "codex");
    assert_eq!(working.calls, 1);
    let lately = working.lately.unwrap();
    let [made] = &lately.calls[..] else { panic!("{lately:?}") };
    assert_eq!(
        (made.id.as_str(), made.before.as_ref().map(|said| said.text.as_str())),
        ("test", Some("I'll run the tests."))
    );
    assert_eq!(made.call, Typed::Command(typed_command("cargo test")));
    let result = made.result.as_ref().unwrap();
    let CallOutput::Command(printed) = &result.output else { panic!("{result:?}") };
    assert!(result.cut && printed.output.len() < long.len(), "cut short");
    assert_eq!(lately.since.map(|said| said.text).as_deref(), Some("They pass."));

    record(&store, &ended("codex", TaskOutcome::Finished, Some("They pass.")));
    let finished = subagent(&store, "codex");
    assert_eq!((finished.lately, finished.calls), (None, 1), "it's only shown while it works");
    record(&store, &exited());
    assert_eq!(subagent(&store, "other").lately, None, "nor once it's gone with its agent");
}
