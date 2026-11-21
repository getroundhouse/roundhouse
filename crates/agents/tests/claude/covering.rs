//! The line said for the agent while it can't answer what was said out loud, with a script standing in for the CLI:
//! both the agent, and the stand-in when it's started on Haiku.

use std::time::Duration;

use agents::claude::spawn_at;
use agents::{AgentOptions, AgentOutput};
use protocol::capability::output::{CoverLine, CoverWait};
use protocol::capability::{Control, Report, prompts::Extras};
use protocol::event::{ContentBlock, EventBody};
use protocol::model::{InteractionMode, Seq};
use protocol::prompt::Prompt;
use protocol::worker::AgentSpec;
use tokio::sync::mpsc;
use tokio::time::timeout;

use super::process::{script, spec};

/// The next of the agent's output that `picked` picks, passing over the rest.
async fn next<T>(output: &mut mpsc::Receiver<AgentOutput>, picked: impl Fn(&AgentOutput) -> Option<T>) -> T {
    loop {
        let item = timeout(Duration::from_secs(20), output.recv()).await.expect("more output").expect("output");
        if let Some(picked) = picked(&item) {
            return picked;
        }
    }
}

/// Something said out loud that the agent hasn't answered when its time is up opens a wait with a line said in it,
/// which a CLI started on Haiku writes, and the agent's answer closes the wait, after the answer, never before. The
/// agent takes it at once, as the CLI says of a prompt sent with a `uuid`, and answers once the test has the line, as a
/// file it makes says. The mode is on from its launch, or as it's told
/// after.
#[tokio::test]
async fn what_was_said_out_loud_and_not_answered_in_time_gets_a_line_until_the_agent_answers() {
    let dir = tempfile::tempdir().unwrap();
    let gate = dir.path().join("answer");
    let body = format!(
        r#"
case "$*" in
*"--model haiku"*)
    read line
    printf '{{"type":"result","subtype":"success","is_error":false,"result":"Hold on, one sec."}}\n' ;;
*)
    printf '{{"type":"system","subtype":"init","session_id":"s"}}\n'
    read line
    uuid=$(printf '%s' "$line" | sed 's/.*"uuid":"\([^"]*\)".*/\1/')
    printf '{{"type":"command_lifecycle","state":"started","command_uuid":"%s","uuid":"u","session_id":"s"}}\n' "$uuid"
    until [ -f '{gate}' ]; do sleep 0.05; done
    printf '{{"type":"assistant","message":{{"id":"m1","content":[{{"type":"text","text":"Found it."}}]}}}}\n'
    exec cat >/dev/null ;;
esac"#,
        gate = gate.display()
    );
    for told_after in [false, true] {
        let (tx, mut rx) = mpsc::channel(64);
        let launched_in = if told_after { InteractionMode::Typing } else { InteractionMode::Conversation };
        let spec = AgentSpec { interaction: launched_in, ..spec(dir.path()) };
        let _ = std::fs::remove_file(&gate);
        let agent = spawn_at(script(dir.path(), &body), spec, tx, AgentOptions::default()).await.unwrap();
        if told_after {
            agent.control(&Control::Interaction { mode: InteractionMode::Conversation }).await.unwrap();
        }
        a_line_until_the_answer(agent.as_ref(), &mut rx, &gate).await;
        agent.stop().await;
    }
}

/// The agent is asked something out loud, and gets a line said for it, which its answer, once `gate` is there, ends.
async fn a_line_until_the_answer(
    agent: &dyn agents::AgentProcess,
    rx: &mut mpsc::Receiver<AgentOutput>,
    gate: &std::path::Path,
) {
    let prompt = Prompt { spoken: true, ..Prompt::new("How's it going?") };
    agent.control(&Control::Prompt { seq: Seq::new(3), prompt, extras: Extras::default() }).await.unwrap();

    let said = next(rx, |item| match item {
        AgentOutput::Report(Report::Covering(line)) => Some(line.clone()),
        _ => None,
    })
    .await;
    assert_eq!(said, CoverLine { wait: CoverWait(1), number: 1, text: "Hold on, one sec.".into() });
    std::fs::write(gate, "").unwrap();
    let answered = next(rx, |item| match item {
        AgentOutput::Event { body: EventBody::Assistant { blocks, .. }, .. } => {
            Some(blocks.iter().any(|block| matches!(block, ContentBlock::Text { .. })))
        }
        AgentOutput::Report(Report::Covered { .. }) => Some(false),
        _ => None,
    })
    .await;
    assert!(answered, "the answer comes before the wait's closing");
    let closed = next(rx, |item| match item {
        AgentOutput::Report(Report::Covered { wait }) => Some(*wait),
        _ => None,
    })
    .await;
    assert_eq!(closed, CoverWait(1));
}
