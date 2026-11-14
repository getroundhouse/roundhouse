//! What a panel asks, which says what it shows and how an answer is sent: the agent's questions, or whether the agent
//! may use a tool. Each way the two differ is one match here, so the panel itself asks either the same way.

use client_core::Approval;
use gpui_kit::{AnyElement, App};
use protocol::event::{ApprovalDecision, EventBody, Question, QuestionAnswer};
use protocol::model::ToolUseId;

use super::tool_use::{self, ToolUse};

pub(crate) enum Asks {
    /// The agent's questions, which want answers.
    Questions(Approval<'static>),
    /// Whether the agent may use a tool, asked as one question that's allowed or denied.
    ToolUse(ToolUse),
}

impl Asks {
    /// What the request `body` asks in the composer's place, or `None` for a plan, whose card is in the transcript
    /// with the plan, and for any other event.
    pub(crate) fn of(body: &EventBody, claims: Option<&protocol::capability::Capabilities>) -> Option<Self> {
        let approval = Approval::of(body)?.into_owned();
        if approval.questions().is_some() {
            return Some(Asks::Questions(approval));
        }
        let typed_request = matches!(body, EventBody::ApprovalRequested { request: Some(_), .. });
        let words = !typed_request
            || claims.is_some_and(|claims| {
                claims.approvals.requests.as_ref().is_some_and(|requests| {
                    requests.parts.contains(&protocol::capability::approvals::DecisionPart::Words)
                })
            });
        ToolUse::read(body, approval, words).map(Asks::ToolUse)
    }

    /// The subagent that asks `body`'s request, as the call that started it, whichever it asks.
    pub(crate) fn asked_by(body: &EventBody) -> Option<&ToolUseId> {
        let EventBody::ApprovalRequested { asked_by, .. } = body else { return None };
        asked_by.as_ref()
    }

    pub(super) fn secret(&self, question: usize) -> bool {
        matches!(self, Asks::Questions(approval) if approval.secret(question))
    }

    pub(crate) fn can_type(&self, question: usize) -> bool {
        match self {
            Asks::Questions(approval) => approval.can_type(question),
            Asks::ToolUse(tool) => tool.can_type,
        }
    }

    /// The questions to show, one at a time.
    pub(super) fn questions(&self) -> Vec<Question> {
        match self {
            Asks::Questions(approval) => approval.questions().unwrap_or_default().to_vec(),
            Asks::ToolUse(tool_use) => vec![tool_use.question().clone()],
        }
    }

    /// The decision that `answers` make, one for each question shown, with one that has nothing in it for a question
    /// left. `None` where they decide nothing, as a tool's use with neither answer chosen does.
    pub(super) fn decision(&self, answers: Vec<QuestionAnswer>) -> Option<ApprovalDecision> {
        match self {
            Asks::Questions(approval) => Some(approval.answered(answers)),
            Asks::ToolUse(tool_use) => tool_use.decision(answers.first()?),
        }
    }

    /// The decision that skips it, for what can be skipped. A tool's use can't, since denying it is one of its
    /// answers.
    pub(super) fn skipped(&self) -> Option<ApprovalDecision> {
        match self {
            Asks::Questions(approval) => Some(approval.refused()),
            Asks::ToolUse(_) => None,
        }
    }

    /// What the typing row says while it's empty.
    pub(super) fn typing_row_says(&self) -> &'static str {
        match self {
            Asks::Questions(_) => "Other…",
            Asks::ToolUse(_) => tool_use::INSTEAD,
        }
    }

    /// Whether a chosen option can be sent from the typing row while nothing's typed there. A question's can, since
    /// Enter there goes on as it does anywhere in the panel. A tool's use can't: the row says it denies, so Enter there
    /// mustn't allow.
    pub(super) fn sends_a_chosen_option_from_the_typing_row(&self) -> bool {
        matches!(self, Asks::Questions(_))
    }

    /// Whether a key that types something other than a row's number takes back the option that's chosen. For a tool's
    /// use it does, since such keys are someone typing who hasn't seen the request, and their Enter mustn't allow it.
    pub(super) fn typing_takes_back_what_is_chosen(&self) -> bool {
        matches!(self, Asks::ToolUse(_))
    }

    /// What shows between the question and its answers: for a tool's use, what the tool was given and why the agent
    /// asks.
    pub(super) fn detail(&self, cx: &App) -> Option<AnyElement> {
        match self {
            Asks::Questions(_) => None,
            Asks::ToolUse(tool_use) => tool_use.detail(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Asks;
    use protocol::capability::{
        Capabilities,
        approvals::{DecisionPart, Requests},
    };
    use protocol::event::EventBody;
    #[test]
    fn a_typed_call_only_offers_a_written_denial_when_the_run_claims_it() {
        let body: EventBody = serde_json::from_value(serde_json::json!({"type":"approval_requested","approval_id":"q",
            "request":{"asks":{"asks":"call","call":{"kind":"other","tool":"sample","input":{}}}}}))
        .unwrap();
        assert!(!Asks::of(&body, None).unwrap().can_type(0));
        let mut claims = Capabilities::default();
        claims.approvals.requests = Some(Requests { parts: Vec::new(), kept: Vec::new() });
        assert!(!Asks::of(&body, Some(&claims)).unwrap().can_type(0));
        claims.approvals.requests.as_mut().unwrap().parts.push(DecisionPart::Words);
        assert!(Asks::of(&body, Some(&claims)).unwrap().can_type(0));
    }
}
