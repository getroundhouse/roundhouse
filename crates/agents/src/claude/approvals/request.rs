//! What the CLI's `can_use_tool` asks, typed: questions, a plan, or the call a tool's use makes, with the rules the CLI
//! suggests keeping, why it asks, and whether it asks whatever the mode.

use super::super::typed;
use super::super::typed::{PLAN, QUESTIONS};
use protocol::capability::approvals::{Asks, Covers, Kept, Request, Rule, RuleEffect};
use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
use serde_json::Value;

/// What the CLI's request `asked`, to use `tool` with `input`, asks. Whatever the CLI sent is taken: a question without
/// its text is left out, as is an option without its label, and a plan without its text is an empty one.
pub(in super::super) fn request(tool: &str, input: &Value, asked: &Value) -> Request {
    let asks = match tool {
        QUESTIONS => Asks::Questions { questions: questions(input) },
        PLAN => Asks::Plan { plan: text_of(input, "plan").unwrap_or_default(), file: text_of(input, "planFilePath") },
        _ => Asks::Call { call: typed::call(tool, input) },
    };
    // A check the CLI makes whatever the mode, such as of a removal it can't rule out, keeps no rule.
    let always = matches!(asks, Asks::Call { .. })
        && (asked.get("suppress_always_allow_rule").and_then(Value::as_bool) == Some(true)
            || asked.get("decision_reason_type").and_then(Value::as_str) == Some("safetyCheck"));
    let suggested = if always { Vec::new() } else { suggested(asked.get("permission_suggestions")) };
    let why = text_of(asked, "decision_reason").filter(|reason| !reason.trim().is_empty());
    Request { asks, suggested, always, why }
}

/// The rules the CLI suggests keeping, from its `permission_suggestions`. Only those that add a rule are rules: the
/// rest, such as changing the mode or adding a directory, the raw line keeps.
fn suggested(suggestions: Option<&Value>) -> Vec<Rule> {
    let suggestions = suggestions.and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    suggestions
        .iter()
        .filter(|suggestion| suggestion.get("type").and_then(Value::as_str) == Some("addRules"))
        .flat_map(|suggestion| {
            let effect = match suggestion.get("behavior").and_then(Value::as_str) {
                Some("allow") => RuleEffect::Allow,
                Some("ask") => RuleEffect::Ask,
                Some("deny") => RuleEffect::Deny,
                _ => RuleEffect::Unknown,
            };
            let kept = kept(suggestion.get("destination").and_then(Value::as_str));
            let rules = suggestion.get("rules").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            rules.iter().filter_map(move |rule| {
                let tool = text_of(rule, "toolName")?;
                let matching = match text_of(rule, "ruleContent") {
                    Some(content) => format!("{tool}({content})"),
                    None => tool.clone(),
                };
                let covers = Covers::Calls { call: typed::kind(&tool) };
                Some(Rule { covers, matching, effect, kept, described: None })
            })
        })
        .collect()
}

/// Where the CLI keeps a rule, by its name for the place.
fn kept(destination: Option<&str>) -> Kept {
    match destination {
        Some("session") => Kept::Conversation,
        Some("localSettings") => Kept::Worktree,
        Some("projectSettings") => Kept::Project,
        Some("userSettings") => Kept::User,
        _ => Kept::Unknown,
    }
}

/// The CLI's name for where `kept` keeps a rule, or `None` for a place it has no name for.
pub(in super::super) fn destination(kept: Kept) -> Option<&'static str> {
    match kept {
        Kept::Conversation => Some("session"),
        Kept::Worktree => Some("localSettings"),
        Kept::Project => Some("projectSettings"),
        Kept::User => Some("userSettings"),
        Kept::Unknown => None,
    }
}

fn questions(input: &Value) -> Vec<Question> {
    let questions = input.get("questions").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
    questions
        .iter()
        .filter_map(|question| {
            let options = question.get("options").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
            let several = question.get("multiSelect").and_then(Value::as_bool).unwrap_or(false);
            Some(Question {
                text: text_of(question, "question")?,
                header: text_of(question, "header"),
                options: options
                    .iter()
                    .filter_map(|option| {
                        Some(QuestionOption {
                            label: text_of(option, "label")?,
                            description: text_of(option, "description"),
                            preview: text_of(option, "preview"),
                        })
                    })
                    .collect(),
                choose: if several { Choose::Several } else { Choose::One },
                // The CLI takes any text as an answer, as its own "Other" does.
                typed: TypedAnswer::Yes,
            })
        })
        .collect()
}

fn text_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use protocol::capability::approvals::{Asks, Covers, Kept, Rule, RuleEffect};
    use protocol::capability::asking::{Choose, Question, QuestionOption, TypedAnswer};
    use protocol::capability::calls::{Call, CallKind};
    use serde_json::{Value, json};

    use super::request;

    fn asks(tool: &str, input: &Value) -> Asks {
        request(tool, input, &json!({})).asks
    }

    #[test]
    fn questions_take_what_the_cli_sends_and_tolerate_what_it_might() {
        let input = json!({"questions":[
            {"question":"Red or blue?","header":"Color","multiSelect":true,
             "options":[{"label":"Red","description":"Warm"},{"label":"Blue","preview":"#00f"},{"description":"no label"}]},
            {"question":"Anything else?"},
            {"header":"no question text"},
        ]});

        let Asks::Questions { questions } = asks("AskUserQuestion", &input) else { panic!() };

        let red = QuestionOption { label: "Red".into(), description: Some("Warm".into()), preview: None };
        let blue = QuestionOption { label: "Blue".into(), description: None, preview: Some("#00f".into()) };
        let question = |text: &str, header: Option<&str>, options, choose| Question {
            text: text.into(),
            header: header.map(str::to_string),
            options,
            choose,
            typed: TypedAnswer::Yes,
        };
        let colors = question("Red or blue?", Some("Color"), vec![red, blue], Choose::Several);
        let anything = question("Anything else?", None, Vec::new(), Choose::One);
        assert_eq!(questions, [colors, anything]);
        assert_eq!(asks("AskUserQuestion", &json!({"something":"else"})), Asks::Questions { questions: Vec::new() });
    }

    #[test]
    fn a_plan_is_its_text_and_file_and_any_other_tool_asks_to_be_used() {
        let plan = json!({"plan": "1. Read\n2. Change", "planFilePath": "/tmp/plan.md"});

        let read = Asks::Plan { plan: "1. Read\n2. Change".into(), file: Some("/tmp/plan.md".into()) };
        assert_eq!(asks("ExitPlanMode", &plan), read);
        assert_eq!(asks("ExitPlanMode", &json!({})), Asks::Plan { plan: String::new(), file: None });
        let Asks::Call { call: Call::Command(command) } = asks("Bash", &json!({"command": "ls"})) else {
            panic!("not the command it makes");
        };
        assert_eq!(command.command, "ls", "and says what call that is");
        let theirs = Call::Mcp { server: "team".into(), tool: "AskUserQuestion".into(), input: json!({}) };
        assert_eq!(asks("mcp__team__AskUserQuestion", &json!({})), Asks::Call { call: theirs }, "only the CLI's own");
    }

    /// As `claude.jsonl` has it: a rule to add, and a directory, which isn't a rule.
    #[test]
    fn the_rules_it_suggests_are_those_it_would_add_in_its_own_form() {
        let asked = json!({"permission_suggestions":[
            {"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"echo 42 > answer.txt"}],"behavior":"allow",
             "destination":"localSettings"},
            {"type":"addDirectories","directories":["/tmp/fixture"],"destination":"session"},
            {"type":"addRules","rules":[{"toolName":"mcp__roundhouse__wait"}],"behavior":"deny","destination":"session"},
        ]});

        let suggested = request("Bash", &json!({"command": "echo 42 > answer.txt"}), &asked).suggested;

        let rule = |covers, matching: &str, effect, kept| Rule {
            covers,
            matching: matching.into(),
            effect,
            kept,
            described: None,
        };
        let command = Covers::Calls { call: CallKind::Command };
        let mcp = Covers::Calls { call: CallKind::Mcp };
        assert_eq!(
            suggested,
            [
                rule(command, "Bash(echo 42 > answer.txt)", RuleEffect::Allow, Kept::Worktree),
                rule(mcp, "mcp__roundhouse__wait", RuleEffect::Deny, Kept::Conversation),
            ]
        );
    }

    /// As `claude_full_access.jsonl` has it.
    #[test]
    fn a_check_it_makes_whatever_the_mode_says_so_and_why_and_keeps_no_rule() {
        let asked = json!({"decision_reason":"Dangerous rm operation","decision_reason_type":"safetyCheck",
            "suppress_always_allow_rule":true,"permission_suggestions":[
                {"type":"addRules","rules":[{"toolName":"Bash"}],"behavior":"allow","destination":"session"}]});

        let checked = request("Bash", &json!({"command": "rm -rf \"$(cat p)\""}), &asked);
        let asked_why = request("Bash", &json!({"command": "ls"}), &json!({"decision_reason": "  "}));

        assert!(checked.always && checked.suggested.is_empty());
        assert_eq!(checked.why.as_deref(), Some("Dangerous rm operation"));
        assert!(!asked_why.always && asked_why.why.is_none(), "words that say nothing aren't why");
    }
}
