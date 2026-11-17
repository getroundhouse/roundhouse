use serde_json::{Value, json};

use super::super::*;
use crate::capability::calls::{Call, CallOutput};
use crate::capability::{Capabilities, approvals};
use crate::capability::{Claimed, approvals as approval_claims, background as background_claims};
use crate::event::{ApprovalOutcome, ExitReason, TaskOutcome};
use crate::event::{ContentBlock, EventBody, SessionEvent};
use crate::fixtures::{event, typed_call, typed_command, typed_result};
use crate::model::{RunId, Seq};
use crate::prompt::Prompt;

/// A call, `id`, to run a command.
pub(super) fn call(id: &str) -> ContentBlock {
    typed_call(id, Call::Command(typed_command("ls")))
}

/// What came back for the call `id`.
pub(super) fn result(id: &str) -> ContentBlock {
    typed_result(id, CallOutput::Words { text: "a.rs".into() }, false)
}

/// `body` at `seq`, from the agent's launch `run`, or about it, as its start and its exit are.
pub(super) fn from_run(run: u64, seq: u64, body: EventBody) -> SessionEvent {
    SessionEvent { run: Some(RunId::new(run)), ..event(Seq::new(seq), body) }
}

/// `body` at `seq`, recorded by the hub, which gives it no run.
pub(super) fn from_hub(seq: u64, body: EventBody) -> SessionEvent {
    event(Seq::new(seq), body)
}

pub(super) fn prompt() -> EventBody {
    EventBody::UserPrompt { client_msg_id: None, prompt: Prompt::new("fix the build") }
}

pub(super) fn taken(prompt: u64) -> EventBody {
    EventBody::PromptTaken { prompt: Seq::new(prompt) }
}

pub(super) fn dropped(prompt: u64) -> EventBody {
    EventBody::PromptDropped { prompt: Seq::new(prompt) }
}

pub(super) fn said(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::Assistant { message_id: None, subagent: None, model: None, blocks, first_index: None }
}

pub(super) fn sent_back(blocks: Vec<ContentBlock>) -> EventBody {
    EventBody::AgentUser { subagent: None, blocks }
}

pub(super) fn asked(id: &str) -> EventBody {
    EventBody::ApprovalRequested {
        request: None,
        approval_id: id.into(),
        tool_name: "Bash".into(),
        tool_use_id: None,
        input: json!({"command": "ls"}),
        description: None,
        suggestions: Box::new(Value::Null),
        asking: None,
        reason: None,
        asked_by: None,
    }
}

pub(super) fn resolved(id: &str) -> EventBody {
    EventBody::ApprovalResolved {
        approval_id: id.into(),
        outcome: ApprovalOutcome::Allowed,
        answers_by_text: None,
        answers_by_position: None,
        by_setting: false,
    }
}

pub(super) fn subagent_ended(call: &str) -> EventBody {
    EventBody::SubagentEnded {
        subagent: call.into(),
        outcome: TaskOutcome::Finished,
        report: None,
        context_tokens: None,
    }
}

pub(super) fn started() -> EventBody {
    EventBody::AgentStarted { pid: 1, resumed: false, build: None, previous_build: None, capabilities: None }
}

pub(super) fn exited() -> EventBody {
    EventBody::AgentExited { reason: ExitReason::Unexpected, code: Some(1), signal: None, stderr_tail: vec![] }
}

/// A run that asks before tools and hands work to subagents, and claims nothing else.
pub(super) fn claims() -> Capabilities {
    Capabilities {
        approvals: approval_claims::Claims {
            requests: Some(approvals::Requests { parts: vec![], kept: vec![] }),
            ..Default::default()
        },
        background: background_claims::Claims { subagents: Some(Claimed {}), ..Default::default() },
        ..Capabilities::default()
    }
}

/// What `log` breaks, where every run claims `claims`.
pub(super) fn breaches(log: &[SessionEvent], claims: &Capabilities) -> Vec<(Law, u64)> {
    by_run(log, |_| claims)
}

pub(super) fn by_run<'c>(log: &[SessionEvent], claims: impl Fn(RunId) -> &'c Capabilities) -> Vec<(Law, u64)> {
    check(log, claims).into_iter().map(|breach| (breach.law, breach.seq.get())).collect()
}
