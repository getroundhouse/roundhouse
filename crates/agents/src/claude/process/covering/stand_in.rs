//! The stand-in: Haiku, which writes the sentence said for the agent while it can't answer. Each sentence comes from a
//! CLI started ahead of time, which has paid for starting, and reading its login, by the time it's asked, and is asked
//! once, so what it's told never piles up. While conversation mode is on, one is kept ready, and asking it starts the
//! next. See `docs/reference/claude-cli.md`.

use std::path::PathBuf;
use std::time::Duration;

use parking_lot::Mutex;
use process::Environment;
use protocol::model::AgentLogin;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::process::{Child, ChildStdin};
use tokio::sync::oneshot;
use tracing::{Instrument as _, debug};

use super::super::start;
use crate::AgentError;
use crate::claude::login_check::{self, Output};
use crate::stdio::GroupGuard;

/// Haiku answers in under a second once its CLI has started, so one taking this long won't be in time to say.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(8);

/// What the stand-in is told it is, as the prototype the owner tried had it, but that it promises nothing about what was
/// said, which Haiku did, as in "I'll check the docs right after".
const STAND_IN: &str = "You speak for a coding agent for a moment, in its own voice, while it's busy or thinking and \
can't answer the person yet. You're told what the person just said, whether the agent is in the middle of something \
else or thinking about what they said, and what it has been doing lately. Reply with one short spoken sentence, in the \
first person as the agent, saying what you're in the middle of right now and asking for a moment. For \
example: \"Hold on, I'm just writing the brief for the parser agent, one sec.\" or \"Hmm, let me think about that for \
a second.\" Never answer, judge, praise or comment on what they said, so never \"good question\" or the like. Never \
mention what they asked for or promise anything about it, so never \"I'll get to that next\". Never mention anything \
the activity doesn't show. The sentence says only what you're doing and then asks them to wait, such as \"one sec\", \
and nothing after that. Say \"a subagent\" or what it's for, never an ID. No quotes and no Markdown: just the sentence.";

/// No tools, MCP servers, slash commands, hooks or project settings, and nothing saved, as the title's call has, at the
/// lowest effort, since it has to answer at once.
fn args() -> Vec<String> {
    [
        "-p",
        "--model",
        "haiku",
        "--effort",
        "low",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "user",
        "--settings",
        r#"{"disableAllHooks":true}"#,
        "--no-session-persistence",
        "--disable-slash-commands",
        "--system-prompt",
        STAND_IN,
    ]
    .map(String::from)
    .to_vec()
}

/// How to start the stand-in's CLI: the agent's own, under the agent's login, with what its worker gives every process.
#[derive(Clone)]
pub(in super::super) struct Starting {
    pub(in super::super) bin: PathBuf,
    pub(in super::super) login: Option<AgentLogin>,
    pub(in super::super) environment: Environment,
}

/// A CLI started and waiting for what to say, in an empty directory of its own, so it reads no project's instructions.
/// Dropping it stops it, and everything it started, and removes its directory.
struct Spare {
    stdin: ChildStdin,
    stdout: Output,
    _child: Child,
    _running: GroupGuard,
    _room: tempfile::TempDir,
}

impl Spare {
    async fn start(starting: &Starting) -> Result<Self, AgentError> {
        let room = tempfile::Builder::new().prefix("roundhouse-cover-").tempdir().map_err(crate::CallFailed::Io)?;
        let mut command = crate::starting::piped(&starting.bin, room.path());
        command.args(args());
        let mut child =
            start(&mut command, &starting.bin, room.path(), starting.login.as_ref(), &starting.environment)?;
        let running = GroupGuard::new(child.id().unwrap_or_default(), None);
        let no_pipes = || AgentError::NoPipes { bin: starting.bin.display().to_string() };
        let mut stdin = child.stdin.take().ok_or_else(no_pipes)?;
        let mut stdout = Output::new(child.stdout.take().ok_or_else(no_pipes)?);
        // Read, so it never waits on a full pipe, and dropped, since what it says is for nobody.
        if let Some(mut stderr) = child.stderr.take() {
            tokio::spawn(async move { stderr.read_to_end(&mut Vec::new()).await });
        }
        if let Some(login) = &starting.login {
            login_check::verify(&mut stdin, &mut stdout, login).await?;
        }
        Ok(Self { stdin, stdout, _child: child, _running: running, _room: room })
    }

    /// Its sentence for `asked`, or `None` if it says nothing that can be said, or nothing in time.
    async fn answer(mut self, asked: &str) -> Option<String> {
        let line = json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": asked}]}});
        let mut written = serde_json::to_vec(&line).ok()?;
        written.push(b'\n');
        self.stdin.write_all(&written).await.ok()?;
        self.stdin.flush().await.ok()?;
        let reading = async {
            let mut buffer = Vec::new();
            while let Some(line) = self.stdout.next(&mut buffer).await {
                let Ok(value) = serde_json::from_str::<Value>(&line) else { continue };
                if value["type"] == "result" {
                    return said(&value);
                }
            }
            None
        };
        tokio::time::timeout(ANSWER_TIMEOUT, reading).await.ok().flatten()
    }
}

/// The sentence a `result` line holds, on one line, unless it failed or said nothing.
fn said(result: &Value) -> Option<String> {
    if result["is_error"].as_bool() != Some(false) {
        return None;
    }
    let text = result["result"].as_str()?.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = text.trim_matches(|c: char| matches!(c, '"' | '“' | '”')).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// A question for a spare, and where its sentence goes.
type Question = (String, oneshot::Sender<Option<String>>);

/// A spare on a task of its own, waiting for its one question. Dropping it stops it.
struct Ready {
    ask: Option<oneshot::Sender<Question>>,
    task: tokio::task::JoinHandle<()>,
}

impl Ready {
    fn start(starting: Starting) -> Self {
        let (ask, asked) = oneshot::channel::<Question>();
        let task = tokio::spawn(
            async move {
                let spare = match Spare::start(&starting).await {
                    Ok(spare) => spare,
                    Err(e) => {
                        return debug!(
                            name: "claude.stand_in_unstarted",
                            error = format!("{e:#}"),
                            "A stand-in didn't start"
                        );
                    }
                };
                if let Ok((question, answer)) = asked.await {
                    let _ = answer.send(spare.answer(&question).await);
                }
            }
            .in_current_span(),
        );
        Self { ask: Some(ask), task }
    }

    /// Its sentence for `asked`, which a spare that couldn't start doesn't have.
    async fn line(mut self, asked: String) -> Option<String> {
        let (answer, answered) = oneshot::channel();
        self.ask.take()?.send((asked, answer)).ok()?;
        // Its task ends once it has answered, so dropping it then stops nothing more.
        answered.await.ok().flatten()
    }
}

impl Drop for Ready {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The stand-in for one agent: a spare CLI kept ready while conversation mode is on, so even the first sentence is
/// quick, and none while it's off, when nobody is listening.
pub(super) struct StandIn {
    starting: Starting,
    ready: Option<Ready>,
    /// Whether to keep a spare ready, as the mode says.
    warm: bool,
}

impl StandIn {
    pub(super) fn new(starting: Starting) -> Self {
        Self { starting, ready: None, warm: false }
    }

    /// Keeps a spare ready from now on, starting one if none is.
    pub(super) fn warm(&mut self) {
        self.warm = true;
        if self.ready.as_ref().is_none_or(|ready| ready.task.is_finished()) {
            self.ready = Some(Ready::start(self.starting.clone()));
        }
    }

    /// Stops the spare kept ready, if there is one, and keeps none from now on.
    pub(super) fn stop(&mut self) {
        self.warm = false;
        self.ready = None;
    }

    /// The spare to ask, which has to start first if none is ready, and starts the next, if one is kept ready, so it's
    /// there for the next time. Taken under the lock, and asked after, so stopping never waits on an answer.
    fn take(&mut self) -> Ready {
        let ready = match self.ready.take() {
            Some(ready) if !ready.task.is_finished() => ready,
            _ => Ready::start(self.starting.clone()),
        };
        if self.warm {
            self.ready = Some(Ready::start(self.starting.clone()));
        }
        ready
    }

    /// The stand-in's sentence for `asked`. One that can't start, or answer in time, says nothing, which costs a
    /// sentence, not the agent's work.
    pub(super) async fn line(stand_in: &Mutex<StandIn>, asked: String) -> Option<String> {
        let ready = stand_in.lock().take();
        ready.line(asked).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Such as the CLI: it reads one question, says its sentence and exits, as the CLI does once its stdin closes. Its
    /// sentence names the start of what it was asked, which shows each question reached a CLI of its own.
    #[cfg(unix)]
    #[tokio::test]
    async fn each_sentence_comes_from_a_spare_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("claude");
        let script = r#"#!/bin/sh
read line
asked=$(printf '%s' "$line" | sed 's/.*"text":"\([a-z]*\).*/\1/')
printf '{"type":"result","is_error":false,"result":"Hold on, %s."}\n' "$asked"
"#;
        process::testing::write_program(&bin, script).unwrap();
        let stand_in = Mutex::new(StandIn::new(Starting { bin, login: None, environment: Environment::NONE }));
        stand_in.lock().warm();

        assert_eq!(StandIn::line(&stand_in, "first".into()).await.as_deref(), Some("Hold on, first."));
        assert_eq!(StandIn::line(&stand_in, "second".into()).await.as_deref(), Some("Hold on, second."), "the next");
        assert!(stand_in.lock().ready.is_some(), "a spare is kept ready while the mode is on");
        stand_in.lock().stop();
        assert!(stand_in.lock().ready.is_none());
        assert_eq!(StandIn::line(&stand_in, "third".into()).await.as_deref(), Some("Hold on, third."), "started then");
        assert!(stand_in.lock().ready.is_none(), "and none kept after");
    }

    #[test]
    fn its_sentence_is_the_result_on_one_line_without_quotes() {
        let result = json!({"type": "result", "is_error": false, "result": "\"Hold on, I'm reading\n the test.\""});

        assert_eq!(said(&result).as_deref(), Some("Hold on, I'm reading the test."));
        assert_eq!(said(&json!({"type": "result", "is_error": true, "result": "Overloaded"})), None);
        assert_eq!(said(&json!({"type": "result", "is_error": false, "result": "  "})), None);
    }
}
