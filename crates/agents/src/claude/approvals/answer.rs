//! What the CLI is told of what the person decided about a request, in its own form: the tool's input for an approval,
//! the answers spliced into it for questions, and the words for a refusal, which are the adapter's to choose when the
//! person gave none.

use std::collections::BTreeMap;

use protocol::event::{ApprovalDecision, Asking, Question, QuestionAnswer};
use serde_json::{Value, json};

/// Told to the agent when the person won't let it use a tool, and said nothing more.
const DENIED: &str = "The user denied this.";
/// Told to the agent when the person won't approve its plan yet, and said nothing more.
const KEEP_PLANNING: &str = "The user wants to keep planning. Ask them what to change.";
/// Told to the agent when the person would rather not answer its questions.
const NOT_ANSWERED: &str = "The user chose not to answer. Carry on if you can, or ask in chat.";

/// A request waiting on the person: the tool's input, which allowing it sends back, and what it asks.
pub(in crate::claude) struct Waiting {
    pub(in crate::claude) input: Value,
    pub(in crate::claude) asking: Asking,
}

/// What the CLI is told for `decision`, as the `response` of its `control_response`, on the request `waiting` was, or
/// on one this process doesn't know, which it allows as asked.
pub(in crate::claude) fn response(decision: &ApprovalDecision, waiting: Option<Waiting>) -> Value {
    let (input, asking) = waiting.map_or((None, None), |waiting| (Some(waiting.input), Some(waiting.asking)));
    match decision {
        ApprovalDecision::Allow { updated_input, updated_permissions } => {
            let mut body = json!({
                "behavior": "allow",
                "updatedInput": updated_input.clone().or(input).unwrap_or_else(|| json!({})),
            });
            if let Some(permissions) = updated_permissions {
                body["updatedPermissions"] = permissions.clone();
            }
            body
        }
        ApprovalDecision::Deny { message, interrupt } => {
            let message = message.as_deref().unwrap_or_else(|| refusal(asking.as_ref()));
            json!({"behavior": "deny", "message": message, "interrupt": interrupt})
        }
        ApprovalDecision::AnswerByText { answers } => answered(input, answers),
        ApprovalDecision::AnswerByPosition { answers } => {
            let questions = match &asking {
                Some(Asking::Questions { questions }) => questions.as_slice(),
                Some(Asking::ToolUse | Asking::Plan { .. } | Asking::Unknown) | None => &[],
            };
            answered(input, &by_text(questions, answers))
        }
    }
}

/// The CLI takes answers as part of the tool's input. Allowing without them tells the agent the person didn't answer.
fn answered(input: Option<Value>, answers: &BTreeMap<String, String>) -> Value {
    let mut input = input.unwrap_or_else(|| json!({}));
    input["answers"] = json!(answers);
    json!({"behavior": "allow", "updatedInput": input})
}

/// The answers as the CLI wants them: by the question's text, with what was chosen and typed for one joined with a comma
/// and a space, and a question with nothing left out. The CLI keys them by text, so of two questions alike, the agent
/// hears the answer to the last.
fn by_text(questions: &[Question], answers: &[QuestionAnswer]) -> BTreeMap<String, String> {
    let said = |(question, answer): (&Question, &QuestionAnswer)| {
        let parts: Vec<&str> = question.said(answer).collect();
        (!parts.is_empty()).then(|| (question.text.clone(), parts.join(", ")))
    };
    questions.iter().zip(answers).filter_map(said).collect()
}

/// What the agent is told when the person says no and gives no words of their own, as fits what was asked.
fn refusal(asking: Option<&Asking>) -> &'static str {
    match asking {
        Some(Asking::Questions { .. }) => NOT_ANSWERED,
        Some(Asking::Plan { .. }) => KEEP_PLANNING,
        Some(Asking::ToolUse | Asking::Unknown) | None => DENIED,
    }
}

#[cfg(test)]
mod tests;
