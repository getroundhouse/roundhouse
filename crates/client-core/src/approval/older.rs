//! Requests for approval recorded before adapters said what each asks, which carry only the agent's tool and its input,
//! as the one adapter of then, Claude's, sent them. This reads what one asks from those, and gives the forms that
//! adapter takes an answer in: by each question's text, and a refusal in words of the client's. It's only for a request
//! with no `asking`, and it stays as Claude's tools were then, whatever they become.

use std::collections::BTreeMap;

use protocol::event::{Asking, Question, QuestionAnswer, QuestionOption};
use serde_json::Value;

/// The tool that asked the person questions.
pub(crate) const QUESTIONS: &str = "AskUserQuestion";
/// The tool that asked to leave plan mode, with the plan.
pub(crate) const PLAN: &str = "ExitPlanMode";

/// Told to the agent when the person wouldn't let it use a tool.
const DENIED: &str = "The user denied this.";
/// Told to the agent when the person wouldn't approve its plan yet.
const KEEP_PLANNING: &str = "The user wants to keep planning. Ask them what to change.";
/// Told to the agent when the person would rather not answer its questions.
const SKIPPED: &str = "The user chose not to answer. Carry on if you can, or ask in chat.";

/// What a request to use `tool` with `input` asked. Whatever was sent is taken: a question without its text is left out,
/// as is an option without its label, and a plan without its text is an empty one.
pub(super) fn asking(tool: &str, input: &Value) -> Asking {
    match tool {
        QUESTIONS => Asking::Questions { questions: questions(input) },
        PLAN => Asking::Plan { plan: text_of(input, "plan").unwrap_or_default() },
        _ => Asking::ToolUse { call: None },
    }
}

fn questions(input: &Value) -> Vec<Question> {
    let questions = input.get("questions").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    questions
        .iter()
        .filter_map(|question| {
            let options = question.get("options").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            Some(Question {
                text: text_of(question, "question")?,
                header: text_of(question, "header"),
                options: options
                    .iter()
                    .filter_map(|option| {
                        Some(QuestionOption {
                            label: text_of(option, "label")?,
                            description: text_of(option, "description"),
                        })
                    })
                    .collect(),
                multiple: question.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect()
}

fn text_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// The answers by each question's text, with what was chosen and typed for one joined with a comma and a space, and a
/// question with nothing left out.
pub(super) fn by_text(questions: &[Question], answers: &[QuestionAnswer]) -> BTreeMap<String, String> {
    let said = |(question, answer): (&Question, &QuestionAnswer)| {
        let parts: Vec<&str> = question.said(answer).collect();
        (!parts.is_empty()).then(|| (question.text.clone(), parts.join(", ")))
    };
    questions.iter().zip(answers).filter_map(said).collect()
}

/// What the agent was told when the person said no, as fit what was asked.
pub(super) fn refusal(asking: &Asking) -> &'static str {
    match asking {
        Asking::Questions { .. } => SKIPPED,
        Asking::Plan { .. } => KEEP_PLANNING,
        Asking::ToolUse { .. } | Asking::Unknown => DENIED,
    }
}
