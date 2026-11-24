//! What a panel asks, which says what it shows and how an answer is sent: the agent's questions, or whether the agent
//! may use a tool. Each way the two differ is one match here, so the panel itself asks either the same way.

use client_core::{Approval, ToolUseAnswer};
use gpui_kit::{AnyElement, App};
use protocol::capability::Capabilities;
use protocol::capability::approvals::{Decision, DecisionPart};
use protocol::capability::asking::{Question, QuestionAnswer};
use protocol::event::EventBody;
use protocol::model::{AnswerSoFar, SoFar, SubagentId};

use super::tool_use::{self, ToolUse};

pub(crate) enum Asks {
    /// The agent's questions, which want answers.
    Questions(Approval<'static>),
    /// Whether the agent may use a tool, asked as one question that's allowed or denied.
    ToolUse(ToolUse),
}

impl Asks {
    /// What the request `body` asks in the composer's place, of a run that claims `claims`, or `None` for a plan,
    /// whose card is in the transcript with the plan, for what this build can't ask, and for any other event. A
    /// tool's use can be denied in words only where the run takes them. Claims that aren't known at all, neither the
    /// run's nor its kind's, as of a kind newer than this build, or before the hub has described its agents, are
    /// taken to take words, as Claude's runs do.
    pub(crate) fn of(body: &EventBody, claims: Option<&Capabilities>) -> Option<Self> {
        let approval = Approval::of(body)?.into_owned();
        if approval.questions().is_some() {
            return Some(Asks::Questions(approval));
        }
        let words = claims.is_none_or(|claims| {
            let requests = claims.approvals.requests.as_ref();
            requests.is_some_and(|requests| requests.parts.contains(&DecisionPart::Words))
        });
        ToolUse::read(body, approval, words).map(Asks::ToolUse)
    }

    /// The subagent that asks `body`'s request, whichever it asks.
    pub(crate) fn asked_by(body: &EventBody) -> Option<&SubagentId> {
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
    pub(super) fn decision(&self, answers: Vec<QuestionAnswer>) -> Option<Decision> {
        match self {
            Asks::Questions(approval) => Some(approval.answered(answers)),
            Asks::ToolUse(tool_use) => tool_use.decision(answers.first()?),
        }
    }

    /// The decision that skips it, for what can be skipped. A tool's use can't, since denying it is one of its
    /// answers.
    pub(super) fn skipped(&self) -> Option<Decision> {
        match self {
            Asks::Questions(approval) => Some(approval.refuse(None)),
            Asks::ToolUse(_) => None,
        }
    }

    /// What's chosen and typed on each question shown, as a draft keeps it: answers to questions as they are, and a
    /// tool's use by the answer chosen, by name, and the words being written to deny it in.
    pub(super) fn drafted(&self, so_far: Vec<AnswerSoFar>) -> SoFar {
        match self {
            Asks::Questions(_) => SoFar::Answers { answers: so_far },
            Asks::ToolUse(_) => {
                let AnswerSoFar { picked, typed, other } = so_far.into_iter().next().unwrap_or_default();
                let chosen = picked.first().and_then(|&at| ToolUseAnswer::at(usize::try_from(at).ok()?));
                SoFar::Use { chosen: chosen.map(ToolUseAnswer::chosen), words: typed, writing: other }
            }
        }
    }

    /// What a draft, `so_far`, says is chosen and typed on each question shown, as [`Asks::drafted`] keeps it. One of
    /// another kind, or that this build can't read, says nothing.
    pub(super) fn answers_drafted(&self, so_far: &SoFar) -> Vec<AnswerSoFar> {
        match (self, so_far) {
            (Asks::Questions(_), SoFar::Answers { answers }) => answers.clone(),
            (Asks::ToolUse(_), SoFar::Use { chosen, words, writing }) => {
                let place = chosen.and_then(ToolUseAnswer::of).map(ToolUseAnswer::place);
                let picked = place.iter().filter_map(|&place| u32::try_from(place).ok()).collect();
                vec![AnswerSoFar { picked, typed: words.clone(), other: *writing }]
            }
            (Asks::Questions(_) | Asks::ToolUse(_), SoFar::Answers { .. } | SoFar::Use { .. } | SoFar::Unknown) => {
                Vec::new()
            }
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
    use protocol::capability::Capabilities;
    use protocol::capability::approvals::{DecisionPart, Requests};
    use protocol::event::EventBody;
    use protocol::fixtures::{approval_request, to_run};
    use protocol::model::{AnswerSoFar, SoFar, UseChoice};

    #[test]
    fn a_call_only_offers_a_written_denial_when_the_run_claims_it_or_nothing_says() {
        let body: EventBody = approval_request("q", to_run("ls")).into();
        assert!(Asks::of(&body, None).unwrap().can_type(0), "a run nothing says the claims of takes words");
        let mut claims = Capabilities::default();
        claims.approvals.requests = Some(Requests { parts: Vec::new(), kept: Vec::new() });
        assert!(!Asks::of(&body, Some(&claims)).unwrap().can_type(0));
        claims.approvals.requests.as_mut().unwrap().parts.push(DecisionPart::Words);
        assert!(Asks::of(&body, Some(&claims)).unwrap().can_type(0));
    }

    #[test]
    fn a_draft_keeps_a_tools_use_by_the_answer_chosen_and_the_words_being_written() {
        let body: EventBody = approval_request("q", to_run("ls")).into();
        let asks = Asks::of(&body, None).unwrap();
        let deny = AnswerSoFar { picked: vec![1], typed: "Use -l".into(), other: true };

        let drafted = asks.drafted(vec![deny.clone()]);

        assert_eq!(drafted, SoFar::Use { chosen: Some(UseChoice::Deny), words: "Use -l".into(), writing: true });
        assert_eq!(asks.answers_drafted(&drafted), [deny]);
        let unknown = SoFar::Use { chosen: Some(UseChoice::Unknown), words: String::new(), writing: false };
        assert_eq!(asks.answers_drafted(&unknown), [AnswerSoFar::default()], "a choice this build doesn't know");
        assert_eq!(asks.answers_drafted(&SoFar::Answers { answers: vec![] }), [], "a draft of another kind");
    }
}
