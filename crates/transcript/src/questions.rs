//! Questions the agent asks the user. They arrive as a tool waiting for approval, but what they need
//! is an answer: allowing one without an answer tells the agent the user didn't reply. While one waits, it's asked in
//! the composer's place, by [`QuestionPanel`], and once it's no longer waiting the transcript draws it as a call, as
//! `rows::asked` says.

use serde_json::Value;

mod choice;
mod panel;

pub use panel::QuestionPanel;
#[cfg(feature = "testing")]
pub(crate) use panel::SETTLE;

pub(crate) const TOOL_NAME: &str = "AskUserQuestion";

/// Told to the agent when the user would rather not pick anything.
const SKIPPED: &str = "The user chose not to answer. Carry on if you can, or ask in chat.";

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Question {
    pub text: String,
    pub header: Option<String>,
    pub options: Vec<QuestionOption>,
    pub multiple: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// Whatever the agent sent. A question with no options can still be answered by typing.
pub(crate) fn parse(input: &Value) -> Vec<Question> {
    let text_of = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse;

    #[test]
    fn takes_what_the_cli_sends_and_tolerates_what_it_might() {
        let input = json!({"questions":[
            {"question":"Red or blue?","header":"Color","multiSelect":true,
             "options":[{"label":"Red","description":"Warm"},{"label":"Blue"},{"description":"no label"}]},
            {"question":"Anything else?"},
            {"header":"no question text"},
        ]});
        let asked = parse(&input);
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0].text, "Red or blue?");
        assert!(asked[0].multiple);
        assert_eq!(asked[0].options.len(), 2);
        assert_eq!(asked[0].options[0].description.as_deref(), Some("Warm"));
        assert!(asked[1].options.is_empty() && !asked[1].multiple);
        assert!(parse(&json!({"something":"else"})).is_empty());
    }
}
