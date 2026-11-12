//! How a request for approval prints: a tool's use by its input, questions with their options numbered, and a plan
//! whole, each with the command that answers it, and once it's closed, what became of it and what was answered.

use client_core::calls::ToolCall;
use client_core::{Answered, Approval};
use protocol::event::{ApprovalOutcome, Asking, EventBody, Question, SessionEvent};

use super::{Printer, about};
use crate::run::short;

impl Printer {
    pub(super) fn approval_requested(&mut self, event: &SessionEvent) {
        let Some(approval) = Approval::of(&event.body) else { return };
        if let EventBody::ApprovalRequested { approval_id, .. } = &event.body
            && let Some(questions) = approval.questions()
        {
            self.asked.insert(approval_id.clone(), questions.to_vec());
        }
        for line in requested(event, &approval) {
            self.line(event.seq, &line);
        }
    }

    pub(super) fn approval_resolved(&mut self, event: &SessionEvent) {
        let EventBody::ApprovalResolved { approval_id, outcome, .. } = &event.body else { return };
        let questions = self.asked.remove(approval_id).unwrap_or_default();
        for line in resolved(approval_id.as_str(), *outcome, Answered::of(&event.body).as_ref(), &questions) {
            self.line(event.seq, &line);
        }
    }
}

/// The lines for the request `event`, which asks what `approval` says.
fn requested(event: &SessionEvent, approval: &Approval) -> Vec<String> {
    let EventBody::ApprovalRequested { approval_id, tool_name, input, description, .. } = &event.body else {
        return Vec::new();
    };
    let session = &event.session_id;
    match approval.asking() {
        Asking::Questions { questions } => {
            let mut lines: Vec<String> = questions.iter().flat_map(question).collect();
            let first = questions.first().map_or("", |question| question.text.as_str());
            lines.push(format!("  roundhouse answer {session} {approval_id} {first:?} <answer>"));
            lines
        }
        Asking::Plan { plan } => {
            let mut lines = vec!["PLAN:".to_string()];
            lines.extend(plan.lines().map(|line| format!("  {line}").trim_end().to_string()));
            lines.push(format!("  roundhouse approve {session} {approval_id}, or with --deny <what to change>"));
            lines
        }
        Asking::ToolUse | Asking::Unknown => {
            let summary = description.clone().or_else(|| ToolCall::read(tool_name, input).summary());
            vec![
                format!("APPROVAL NEEDED: {}", about(tool_name, summary)),
                format!("  roundhouse approve {session} {approval_id}"),
            ]
        }
    }
}

/// A question, under its header, and its options numbered, with what each means.
fn question(question: &Question) -> Vec<String> {
    let header = question.header.as_ref().map_or_else(String::new, |header| format!("{header}: "));
    let mut lines = vec![format!("QUESTION: {header}{}", question.text)];
    for (number, option) in (1..).zip(&question.options) {
        let means = option.description.as_ref().map_or_else(String::new, |description| format!(": {description}"));
        lines.push(format!("  {number}. {}{means}", option.label));
    }
    if question.multiple {
        lines.push("  (any number of these)".to_string());
    }
    lines
}

/// What became of the approval `id`, and what was answered to each of `questions`, which are the ones it asked, if this
/// printed it asking.
fn resolved(id: &str, outcome: ApprovalOutcome, answered: Option<&Answered>, questions: &[Question]) -> Vec<String> {
    let mut lines = vec![format!("approval {} {outcome:?}", short(id))];
    let Some(answered) = answered else { return lines };
    for (at, question) in questions.iter().enumerate() {
        let answer = answered.to(at, question);
        lines.push(format!("  {}: {}", question.text, answer.as_deref().unwrap_or("not answered")));
    }
    if questions.is_empty()
        && let Answered::ByText(answers) = answered
    {
        lines.extend(answers.iter().map(|(question, answer)| format!("  {question}: {answer}")));
    }
    lines
}

#[cfg(test)]
mod tests;
