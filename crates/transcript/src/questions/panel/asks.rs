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
    pub(crate) fn of(body: &EventBody) -> Option<Self> {
        let approval = Approval::of(body)?.into_owned();
        if approval.questions().is_some() {
            return Some(Asks::Questions(approval));
        }
        ToolUse::read(body, approval).map(Asks::ToolUse)
    }

    /// The subagent that asks `body`'s request, as the call that started it, whichever it asks.
    pub(crate) fn asked_by(body: &EventBody) -> Option<&ToolUseId> {
        let EventBody::ApprovalRequested { asked_by, .. } = body else { return None };
        asked_by.as_ref()
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
