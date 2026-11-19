//! Made-up events and logs, for tests that read the log the way drawing does.

use client_core::session_log::SessionLog;
use protocol::capability::approvals::Asks;
use protocol::capability::calls::{Call, CallOutput};
use protocol::event::{EventBody, SessionEvent, TurnOutcome};
use protocol::fixtures;
pub(crate) use protocol::fixtures::event;
use protocol::model::{Rev, Seq, Subagent, SubagentStatus};
use protocol::prompt::Prompt;
use protocol::prompt::PromptFrom;

/// The record of the subagent the call `call` started, in the background, at `status`, with nothing else known about
/// it yet. A test sets what it's about.
pub(crate) fn record(call: &str, status: SubagentStatus) -> Subagent {
    Subagent { in_background: true, status, rev: Rev::new(1), ..protocol::fixtures::subagent(call) }
}

pub(crate) fn log(events: impl IntoIterator<Item = SessionEvent>) -> SessionLog {
    let mut log = SessionLog::new(Seq::default());
    for event in events {
        log.apply_event(event);
    }
    log
}

/// The agent running a command for each of `ids` at once.
pub(crate) fn calls(ids: &[&str]) -> EventBody {
    calls_of(&Call::Command(fixtures::typed_command("make")), ids)
}

/// The agent handing work to a subagent for each of `ids` at once.
pub(crate) fn subagents(ids: &[&str]) -> EventBody {
    calls_of(&Call::Subagent(fixtures::typed_subagent("Find where the tests live.")), ids)
}

/// The agent asking the advisor once for each of `ids`, at once.
pub(crate) fn advised(ids: &[&str]) -> EventBody {
    calls_of(&Call::Advisor, ids)
}

/// The agent making `call` once for each of `ids`, at once.
fn calls_of(call: &Call, ids: &[&str]) -> EventBody {
    let blocks = ids.iter().map(|id| fixtures::typed_call(id, call.clone())).collect();
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None }
}

/// `body` as the subagent started by the call `parent` sends it.
pub(crate) fn within(parent: &str, mut body: EventBody) -> EventBody {
    if let EventBody::Assistant { subagent, .. } | EventBody::AgentUser { subagent, .. } = &mut body {
        *subagent = Some(parent.into());
    }
    body
}

/// `body`'s blocks as the agent sends them itself, as it does the result of a tool the API runs, such as the advisor.
pub(crate) fn from_the_agent(body: EventBody) -> EventBody {
    let EventBody::AgentUser { subagent, blocks } = body else { return body };
    EventBody::Assistant { message_id: None, subagent, model: None, blocks, first_index: None }
}

/// The result of the call `id`.
pub(crate) fn result(id: &str) -> EventBody {
    let block = fixtures::typed_result(id, CallOutput::Words { text: format!("from {id}") }, false);
    EventBody::AgentUser { subagent: None, blocks: vec![block] }
}

/// The agent asking the person whether to go on, under the approval `id`.
pub(crate) fn asked_a_question(id: &str) -> EventBody {
    asking(id, fixtures::asking(vec![fixtures::question("Go on?", &[], false)]))
}

/// The agent asking to run a command, under the approval `id`.
pub(crate) fn asked_to_use(id: &str) -> EventBody {
    asking(id, fixtures::to_run("ls"))
}

/// The agent asking `asks` under the approval `id`.
pub(crate) fn asking(id: &str, asks: Asks) -> EventBody {
    fixtures::approval_request(id, asks).into()
}

/// The person's prompt, as the hub recorded it.
pub(crate) fn prompt(text: &str) -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new(text), from: PromptFrom::Person }
}

/// A request to the model that failed on a limit, the agent's own, or the subagent's that `parent` started.
pub(crate) fn failed_request(parent: Option<&str>) -> EventBody {
    protocol::fixtures::request_failed("You've reached your limit.", parent)
}

/// A turn that failed on the agent's own failed request.
pub(crate) fn turn_ended() -> EventBody {
    let mut ended = protocol::fixtures::turn_ended(TurnOutcome::Error);
    let EventBody::TurnEnded { request_failed, .. } = &mut ended else { unreachable!() };
    *request_failed = true;
    ended
}
