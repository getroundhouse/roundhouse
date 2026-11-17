use protocol::capability::approvals::{AfterDenial, Covers, Decision, Kept, Remember, Rule, RuleEffect};
use protocol::capability::asking::{PlanContext, QuestionAnswer};
use protocol::capability::calls::CallKind;
use protocol::fixtures::passed_decisions;
use serde_json::{Value, json};

use super::{DENIED, KEEP_PLANNING, NOT_ANSWERED, Waiting, response};
use crate::claude::approvals::request;
use crate::claude::claims::claims;

/// The CLI's input for two questions, the second of which takes several answers.
fn two_questions() -> Value {
    json!({"questions": [
        {"question": "Red or blue?", "options": [{"label": "Red"}, {"label": "Blue"}]},
        {"question": "Which days?", "multiSelect": true,
         "options": [{"label": "Monday"}, {"label": "Wednesday"}, {"label": "Friday"}]},
    ]})
}

fn waiting(tool: &str, input: Value) -> Waiting {
    Waiting { asks: request(tool, &input, &json!({})).asks, input }
}

fn answer(picked: &[u32], typed: Option<&str>) -> QuestionAnswer {
    QuestionAnswer { picked: picked.to_vec(), typed: typed.map(str::to_string) }
}

fn allow(remember: Remember) -> Decision {
    Decision::Allow { remember }
}

fn deny(words: Option<&str>, then: AfterDenial) -> Decision {
    Decision::Deny { words: words.map(str::to_string), then, keep: None }
}

#[test]
fn answers_reach_the_cli_by_the_questions_text_in_the_tools_input() {
    let answers = vec![answer(&[1], None), answer(&[0, 2], Some("Sunday"))];

    let told = response(&Decision::Answer { answers }, Some(&waiting("AskUserQuestion", two_questions()))).unwrap();

    assert_eq!(told["behavior"], "allow");
    assert_eq!(told["updatedInput"]["questions"], two_questions()["questions"], "the input goes back as it came");
    assert_eq!(
        told["updatedInput"]["answers"],
        json!({"Red or blue?": "Blue", "Which days?": "Monday, Friday, Sunday"})
    );
}

#[test]
fn a_question_left_without_an_answer_is_left_out_and_an_option_it_doesnt_offer_is_ignored() {
    let answers = vec![QuestionAnswer::default(), answer(&[9, 1], None)];

    let told = response(&Decision::Answer { answers }, Some(&waiting("AskUserQuestion", two_questions()))).unwrap();

    assert_eq!(told["updatedInput"]["answers"], json!({"Which days?": "Wednesday"}));
}

#[test]
fn saying_no_without_words_tells_the_agent_what_fits_what_was_asked() {
    let told = |decision: &Decision, waiting: Option<&Waiting>| {
        response(decision, waiting).unwrap()["message"].as_str().unwrap().to_string()
    };
    let no = deny(None, AfterDenial::Continue);
    let none_answered = Decision::Answer { answers: vec![QuestionAnswer::default(), QuestionAnswer::default()] };

    assert_eq!(told(&none_answered, Some(&waiting("AskUserQuestion", two_questions()))), NOT_ANSWERED);
    let plan = waiting("ExitPlanMode", json!({"plan": "1. Read"}));
    assert_eq!(told(&Decision::KeepPlanning { words: None }, Some(&plan)), KEEP_PLANNING);
    assert_eq!(told(&no, Some(&waiting("Bash", json!({"command": "ls"})))), DENIED);
    assert_eq!(told(&no, None), DENIED, "a request this process doesn't know");
}

#[test]
fn the_persons_own_words_are_what_the_agent_is_told() {
    let no = deny(Some("Not on main"), AfterDenial::Stop);

    let told = response(&no, Some(&waiting("Bash", json!({"command": "git push"})))).unwrap();

    assert_eq!(told, json!({"behavior": "deny", "message": "Not on main", "interrupt": true}));
    let smaller = Decision::KeepPlanning { words: Some("Smaller".into()) };
    let plan = waiting("ExitPlanMode", json!({"plan": "1. Read"}));
    assert_eq!(response(&smaller, Some(&plan)).unwrap()["message"], "Smaller");
}

#[test]
fn allowing_sends_the_input_back_as_it_was_asked() {
    let plan = waiting("ExitPlanMode", json!({"plan": "1. Read"}));
    let approve = Decision::ApprovePlan { context: PlanContext::Kept };
    let removal = waiting("Bash", json!({"command": "rm -rf out", "description": "Clean", "timeout": 5000}));

    assert_eq!(response(&approve, Some(&plan)).unwrap()["updatedInput"], json!({"plan": "1. Read"}));
    assert_eq!(response(&allow(Remember::Once), None).unwrap()["updatedInput"], json!({}));
    assert_eq!(
        response(&allow(Remember::Once), Some(&removal)).unwrap()["updatedInput"],
        json!({"command": "rm -rf out", "description": "Clean", "timeout": 5000})
    );
}

#[test]
fn a_kept_rule_goes_to_the_cli_as_the_update_it_suggested() {
    let rule = |matching: &str, kept| Rule {
        covers: Covers::Calls { call: CallKind::Command },
        matching: matching.into(),
        effect: RuleEffect::Allow,
        kept,
        described: None,
    };
    let keeping = |rule| response(&allow(Remember::Rule(rule)), Some(&waiting("Bash", json!({"command": "ls"}))));

    assert_eq!(
        keeping(rule("Bash(npm test:*)", Kept::Worktree)).unwrap()["updatedPermissions"],
        json!([{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"npm test:*"}],"behavior":"allow",
            "destination":"localSettings"}])
    );
    assert_eq!(
        keeping(rule("mcp__team__wait", Kept::Conversation)).unwrap()["updatedPermissions"][0]["rules"],
        json!([{"toolName":"mcp__team__wait"}])
    );
    assert_eq!(keeping(rule("Bash", Kept::Unknown)), None, "a place the CLI keeps none");
    let once_more = response(&allow(Remember::Conversation), Some(&waiting("Bash", json!({"command": "ls"}))));
    assert_eq!(once_more, None, "the CLI keeps no rule that's only for this use");
}

/// Every decision the hub passes on, on each kind of request the CLI makes, is one the CLI is told, so the hub never
/// records a decision the agent never hears.
#[test]
fn every_decision_the_hub_passes_on_reaches_the_cli() {
    let suggests = json!({"permission_suggestions": [{"type": "addRules", "rules": [{"toolName": "Bash",
        "ruleContent": "ls"}], "behavior": "allow", "destination": "localSettings"}]});
    let asked = [
        ("Bash", json!({"command": "ls"}), suggests),
        ("Bash", json!({"command": "rm -rf out"}), json!({"decision_reason_type": "safetyCheck"})),
        ("Write", json!({"file_path": "notes.md", "content": "Notes"}), json!({})),
        ("AskUserQuestion", two_questions(), json!({})),
        ("ExitPlanMode", json!({"plan": "1. Read"}), json!({})),
    ];

    for (tool, input, asked) in asked {
        let request = request(tool, &input, &asked);
        let passed = passed_decisions(&request, &claims());
        assert!(!passed.is_empty(), "{tool} takes something");
        let waiting = Waiting { input, asks: request.asks };
        for decision in passed {
            assert!(response(&decision, Some(&waiting)).is_some(), "{decision:?} on {tool}");
        }
    }
}
