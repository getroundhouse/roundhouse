//! What a request for approval asked, typed, from the forms Claude's adapter recorded it in before it typed the call one
//! asks to make: one that asked to use a tool said only what kind of call that was, and one from before requests said
//! what they ask said only the tool and its input.

use protocol::event::{Asking, Question, QuestionOption};
use serde_json::Value;

use super::calls::{self, PLAN, QUESTIONS};

/// Types what `body`, a request for approval as JSON, asks, where it's in one of the older forms, and says whether it
/// was. A request in a form of its own, as every other agent's is, and one that asks in a form this reads, are left
/// as they are.
pub(super) fn type_request(body: &mut Value) -> bool {
    if body.get("request").is_some_and(|request| !request.is_null()) {
        return false;
    }
    let tool = body.get("tool_name").and_then(Value::as_str).unwrap_or_default();
    let input = body.get("input").unwrap_or(&Value::Null);
    let asking = match body.get("asking") {
        None | Some(Value::Null) => asked(tool, input),
        Some(asking)
            if asking.get("kind").and_then(Value::as_str) == Some("tool_use") && asking.get("typed").is_none() =>
        {
            Asking::ToolUse { typed: Some(calls::call(tool, input)) }
        }
        Some(_) => return false,
    };
    let (Ok(asking), Some(body)) = (serde_json::to_value(asking), body.as_object_mut()) else { return false };
    body.insert("asking".to_owned(), asking);
    true
}

/// What a request to use `tool` with `input` asked. Whatever was sent is taken: a question without its text is left
/// out, as is an option without its label, and a plan without its text is an empty one.
fn asked(tool: &str, input: &Value) -> Asking {
    match tool {
        QUESTIONS => Asking::Questions { questions: questions(input) },
        PLAN => Asking::Plan { plan: text_of(input, "plan").unwrap_or_default() },
        _ => Asking::ToolUse { typed: Some(calls::call(tool, input)) },
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
