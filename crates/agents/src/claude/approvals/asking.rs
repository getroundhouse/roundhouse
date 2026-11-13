use super::super::kinds;
use super::super::kinds::{PLAN, QUESTIONS};
use protocol::event::{Asking, Question, QuestionOption};
use serde_json::Value;

/// What a request to use `tool` with `input` asks. Whatever the CLI sent is taken: a question without its text is left
/// out, as is an option without its label, and a plan without its text is an empty one.
pub(in super::super) fn asking(tool: &str, input: &Value) -> Asking {
    match tool {
        QUESTIONS => Asking::Questions { questions: questions(input) },
        PLAN => Asking::Plan { plan: text_of(input, "plan").unwrap_or_default() },
        _ => Asking::ToolUse { call: Some(kinds::of(tool)) },
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

#[cfg(test)]
mod tests {
    use protocol::event::{Asking, CallKind, Question, QuestionOption};
    use serde_json::json;

    use super::asking;

    #[test]
    fn questions_take_what_the_cli_sends_and_tolerate_what_it_might() {
        let input = json!({"questions":[
            {"question":"Red or blue?","header":"Color","multiSelect":true,
             "options":[{"label":"Red","description":"Warm"},{"label":"Blue"},{"description":"no label"}]},
            {"question":"Anything else?"},
            {"header":"no question text"},
        ]});

        let Asking::Questions { questions } = asking("AskUserQuestion", &input) else { panic!() };

        let red = QuestionOption { label: "Red".into(), description: Some("Warm".into()) };
        let blue = QuestionOption { label: "Blue".into(), description: None };
        let colors = Question {
            text: "Red or blue?".into(),
            header: Some("Color".into()),
            options: vec![red, blue],
            multiple: true,
        };
        let anything = Question { text: "Anything else?".into(), header: None, options: Vec::new(), multiple: false };
        assert_eq!(questions, [colors, anything]);
        assert_eq!(
            asking("AskUserQuestion", &json!({"something":"else"})),
            Asking::Questions { questions: Vec::new() }
        );
    }

    #[test]
    fn a_plan_is_its_text_and_any_other_tool_asks_to_be_used() {
        let plan = json!({"plan": "1. Read\n2. Change", "planFilePath": "/tmp/plan.md"});

        assert_eq!(asking("ExitPlanMode", &plan), Asking::Plan { plan: "1. Read\n2. Change".into() });
        assert_eq!(asking("ExitPlanMode", &json!({})), Asking::Plan { plan: String::new() });
        let command = Asking::ToolUse { call: Some(CallKind::Command) };
        assert_eq!(asking("Bash", &json!({"command": "ls"})), command, "and says what kind of call that is");
        let theirs = CallKind::Other { server: Some("team".into()), tool: "AskUserQuestion".into() };
        assert_eq!(
            asking("mcp__team__AskUserQuestion", &json!({})),
            Asking::ToolUse { call: Some(theirs) },
            "only the CLI's own"
        );
    }
}
