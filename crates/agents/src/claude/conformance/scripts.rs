//! What Claude's scripted model says in each scenario: what a real model does with Claude's tools, from what the CLI
//! sends it.

use serde_json::json;

use super::messages::{Block, Reply, Script};
use crate::conformance::scenario::Scenario;
use crate::conformance::scenarios::{
    ASKED, CANCEL_COMMAND, ECHO, PLAIN_TURN, PLAN, PLAN_APPROVED, READY, RESUME, SLOW, THINKING, THOUGHT, WORD,
};

/// Each scenario's script.
static SCRIPTS: &[(&Scenario, Script)] = &[
    (&PLAIN_TURN, |request| match request.tool_result() {
        Some(printed) => Reply::text(format!("It printed: {}", printed.trim())),
        None if request.offers("Bash") => {
            Reply::call("Bash", json!({"command": ECHO, "description": "Print a greeting"}))
        }
        None => Reply::text("There's no Bash tool."),
    }),
    (&CANCEL_COMMAND, |request| {
        if request.last_text().contains(READY) {
            Reply::text("ready")
        } else if request.tool_result().is_some() {
            Reply::text("Stopped.")
        } else {
            Reply::call("Bash", json!({"command": SLOW, "description": "Wait"}))
        }
    }),
    (&RESUME, |request| {
        if !request.last_text().contains(ASKED) {
            Reply::text("noted")
        } else if request.says(WORD) {
            Reply::text(WORD)
        } else {
            Reply::text("I don't know.")
        }
    }),
    (&THINKING, |_| Reply(vec![Block::Thinking(THOUGHT.into()), Block::Text("Answer.".into())])),
    // The CLI reads the plan from the file plan mode names, and offers `ExitPlanMode` only once the model has loaded
    // it with `ToolSearch`, so the model does both first, as a real one does.
    (&PLAN_APPROVED, |request| {
        if request.called("ExitPlanMode") {
            Reply::text("approved")
        } else if request.offers("ExitPlanMode") {
            Reply::call("ExitPlanMode", json!({}))
        } else if request.called("Write") {
            Reply::call("ToolSearch", json!({"query": "select:ExitPlanMode", "max_results": 1}))
        } else if let Some(path) = request.word_after("create your plan at ") {
            Reply::call("Write", json!({"file_path": path, "content": PLAN}))
        } else {
            Reply::text("Plan mode named no plan file.")
        }
    }),
];

/// `scenario`'s script, if it has one.
pub(super) fn script(scenario: &Scenario) -> Option<Script> {
    SCRIPTS.iter().find(|(scripted, _)| std::ptr::eq(*scripted, scenario)).map(|(_, script)| *script)
}

#[cfg(test)]
mod tests {
    use crate::conformance::scenario::Against;
    use crate::conformance::scenarios::ALL;

    #[test]
    fn every_scenario_on_the_scripted_model_has_a_script() {
        let missing: Vec<_> = ALL
            .iter()
            .filter(|scenario| scenario.against.contains(&Against::ScriptedModel) && super::script(scenario).is_none())
            .map(|scenario| scenario.name)
            .collect();

        assert!(missing.is_empty(), "{missing:?}");
    }
}
