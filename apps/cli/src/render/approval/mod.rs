//! How a request for approval prints: a tool's use by what it's for and what the tool was given, with why the agent
//! asks and which subagent does, questions with their options numbered, and a plan whole, each with the command that
//! answers it, and once it's closed, what became of it and what was answered.

use client_core::calls::first_line;
use client_core::{Answered, Approval, Given, ToolUseAsked};
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
    let EventBody::ApprovalRequested { approval_id, .. } = &event.body else { return Vec::new() };
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
        Asking::ToolUse { .. } | Asking::Unknown => tool_use(event),
    }
}

/// The lines for a request to use a tool: the tool and what the use is for, then what the tool was given where the
/// first line doesn't already say all of it, why the agent asks, and the subagent that asks, by the call that started
/// it. An edit or a write is given by its file, then what it takes out and puts in.
fn tool_use(event: &SessionEvent) -> Vec<String> {
    let (EventBody::ApprovalRequested { approval_id, .. }, Some(asked)) = (&event.body, ToolUseAsked::of(&event.body))
    else {
        return Vec::new();
    };
    let said = match &asked.given {
        Some(Given::Command(command)) => Some(*command),
        Some(Given::MainInput(input)) => Some(input.as_str()),
        Some(given @ (Given::Edit(_) | Given::Write(_))) => given.path(),
        None => None,
    };
    let first = asked.about.or(said.map(first_line)).map(str::to_string);
    let mut lines = vec![format!("APPROVAL NEEDED: {}", about(&asked.tool, first.clone()))];
    let marked = |mark: &str, text: &str| -> Vec<String> {
        text.lines().map(|line| format!("  {mark} {line}").trim_end().to_string()).collect()
    };
    match &asked.given {
        Some(Given::Edit(edit)) => {
            lines.extend(asked.about.map(|_| format!("  file: {}", edit.path)));
            for (old, new) in &edit.edits {
                lines.extend(marked("-", old).into_iter().chain(marked("+", new)));
            }
        }
        Some(Given::Write(write)) => {
            lines.extend(asked.about.map(|_| format!("  file: {}", write.path)));
            lines.extend(marked("+", write.content));
        }
        Some(Given::Command(_) | Given::MainInput(_)) | None => {
            if let Some(said) = said.filter(|said| first.as_deref() != Some(*said)) {
                lines.extend(marked("$", said));
            }
        }
    }
    lines.extend(asked.reason.iter().map(|reason| format!("  why: {reason}")));
    lines.extend(asked.asked_by.iter().map(|call| format!("  asked by the subagent {call} started")));
    lines.push(format!("  roundhouse approve {} {approval_id}", event.session_id));
    lines
}

/// A question, under its header, and its options numbered, with what each means.
fn question(question: &Question) -> Vec<String> {
    let header = question.header.as_ref().map_or_default(|header| format!("{header}: "));
    let mut lines = vec![format!("QUESTION: {header}{}", question.text)];
    for (number, option) in (1..).zip(&question.options) {
        let means = option.description.as_ref().map_or_default(|description| format!(": {description}"));
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
