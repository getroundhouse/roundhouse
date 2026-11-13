//! Subagents that each ask to run a command, one after another, each while those before it still wait. Each request
//! names the task its subagent runs as, in `agent_id`, and no parent, as the one captured from the real CLI does, in
//! `claude_full_access.jsonl`. That capture has one subagent, in the foreground, so several waiting at once, and one
//! asking from the background, are taken to be the same and weren't seen. Each can be answered in any order, at any
//! time. A subagent runs its command if it's allowed, and its task ends either way.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use serde_json::{Value, json};

use crate::tools::{asks_before, execute, request_to_run};
use crate::turn::{Turn, state_event};
use crate::{Input, Prompt, background, emit};

/// Between one subagent asking and the next starting, so the first is being asked when the second's request comes.
const ASKS_EVERY: Duration = Duration::from_millis(300);

/// A subagent with a request waiting.
struct Asking {
    /// The call that started it.
    agent: String,
    task: String,
    /// Its own call, which it asks to make.
    call: String,
    request_id: String,
}

/// Starts as many subagents as `spec` says, as `N:COMMAND`, each for `Check N`. Returns whether the turn was
/// interrupted while any waited.
pub(crate) fn start(
    turn: &mut Turn,
    spec: &str,
    rx: &Receiver<Input>,
    queued: &mut VecDeque<Prompt>,
    eof: &mut bool,
) -> bool {
    let (count, command) = spec.split_once(':').unwrap();
    assert!(asks_before(command), "{command} runs unasked, so nothing would wait");
    let mut waiting: Vec<Asking> = Vec::new();
    let mut to_start = 1..=count.parse::<usize>().unwrap();
    waiting.extend(to_start.next().map(|n| ask(turn, n, command)));
    while !waiting.is_empty() || !to_start.is_empty() {
        match rx.recv_timeout(ASKS_EVERY) {
            // Nothing came, so the next subagent starts, if there's one.
            Err(RecvTimeoutError::Timeout) => waiting.extend(to_start.next().map(|n| ask(turn, n, command))),
            Ok(Input::Approval(response)) => answer(turn, &mut waiting, &response, command),
            Ok(Input::Interrupt) => {
                for asking in &waiting {
                    emit(turn.out, json!({"type":"control_cancel_request","request_id":asking.request_id}));
                    background::unlisted(turn.config, turn.out, &asking.task);
                }
                return true;
            }
            Ok(Input::Prompt(prompt) | Input::Resume(prompt)) => queued.push_back(prompt),
            Ok(Input::Eof) | Err(RecvTimeoutError::Disconnected) => {
                *eof = true;
                return true;
            }
        }
    }
    state_event(turn.config, turn.out, "running");
    false
}

/// Starts the subagent `Check n` in the background, which makes its call and asks to be allowed it.
fn ask(turn: &mut Turn, n: usize, command: &str) -> Asking {
    let description = format!("Check {n}");
    let prompt = format!("{description}.");
    let input = json!({"description":description,"subagent_type":"general-purpose","prompt":prompt});
    let agent = turn.named_tool_use("Agent", input);
    let task = format!("task_{agent}");
    background::listed(turn.config, turn.out, &task, "local_agent", &description);
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":agent,"description":description,
        "task_type":"local_agent","subagent_type":"general-purpose","prompt":prompt,"is_backgrounded":true,
        "session_id":turn.config.session_id}),
    );
    turn.tool_result(&agent, "Async agent launched successfully.", false);
    let call = Turn { parent: Some(&agent), ..*turn }.tool_use(command);
    let (request_id, request) = request_to_run(&call, command, Some(&task));
    // Only the first, since nothing says the CLI's state changes again while one already waits.
    if n == 1 {
        state_event(turn.config, turn.out, "requires_action");
    }
    emit(turn.out, json!({"type":"control_request","request_id":request_id,"request":request}));
    Asking { agent, task, call, request_id }
}

/// Takes the answer to one of the requests waiting: its subagent's call fails with what it's told, or runs, and its
/// task ends.
fn answer(turn: &Turn, waiting: &mut Vec<Asking>, response: &Value, command: &str) {
    let answered = waiting.iter().position(|asking| response["request_id"] == asking.request_id.as_str());
    let asking = waiting.remove(answered.unwrap());
    let mut within = Turn { parent: Some(&asking.agent), ..*turn };
    if response["response"]["behavior"] == "deny" {
        let message = response["response"]["message"].as_str().unwrap_or("denied");
        within.denied_result(&asking.call, message);
    } else {
        execute(&mut within, &asking.call, command);
    }
    emit(
        turn.out,
        json!({"type":"system","subtype":"task_notification","task_id":asking.task,"tool_use_id":asking.agent,
        "status":"completed","summary":"Done.","session_id":turn.config.session_id}),
    );
    background::unlisted(turn.config, turn.out, &asking.task);
}
