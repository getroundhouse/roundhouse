use protocol::capability::approvals::{AfterDenial, Asks, Decision, Remember, Rule, RuleEffect};
use protocol::capability::asking::{self, PlanContext};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use super::super::request::destination;

/// Told to the agent when the person won't let it use a tool, and said nothing more.
pub(super) const DENIED: &str = "The user denied this.";
/// Told to the agent when the person won't approve its plan yet, and said nothing more.
pub(super) const KEEP_PLANNING: &str = "The user wants to keep planning. Ask them what to change.";
/// Told to the agent when the person would rather not answer its questions.
pub(super) const NOT_ANSWERED: &str = "The user chose not to answer. Carry on if you can, or ask in chat.";

/// A request waiting on the person: the input of the tool it asks to use, which allowing it sends back, and what it
/// asks.
pub(in crate::claude) struct Waiting {
    pub(in crate::claude) input: Value,
    pub(in crate::claude) asks: Asks,
}

/// Whether `text`, what a call the person denied told the agent, is this adapter's own words for a denial, which it
/// sends when the person said nothing, rather than theirs.
pub(in crate::claude) fn denied_in_its_own_words(text: &str) -> bool {
    text == DENIED
}

/// What the CLI is told for `decision`, as the `response` of its `control_response`, on the request `waiting` was, or
/// on one this process doesn't know, which it allows or refuses as asked. `None` for a decision the CLI has no form
/// for, such as a use allowed for the conversation, or a rule kept where the CLI keeps none.
pub(in crate::claude) fn response(decision: &Decision, waiting: Option<&Waiting>) -> Option<Value> {
    let input = || waiting.map_or_else(|| json!({}), |waiting| waiting.input.clone());
    let asks = waiting.map(|waiting| &waiting.asks);
    Some(match decision {
        Decision::Allow { remember } => {
            let mut body = json!({"behavior": "allow", "updatedInput": input()});
            match remember {
                Remember::Once => {}
                Remember::Rule(rule) => body["updatedPermissions"] = json!([kept(rule)?]),
                Remember::Conversation | Remember::Unknown => return None,
            }
            body
        }
        // Approving it leaves plan mode for the mode before, which is the only other one a chat can have.
        Decision::ApprovePlan { context: PlanContext::Kept, .. } => {
            json!({"behavior": "allow", "updatedInput": input()})
        }
        Decision::Deny { words, then, keep: None } => {
            let interrupt = match then {
                AfterDenial::Continue => false,
                AfterDenial::Stop => true,
                AfterDenial::Unknown => return None,
            };
            let message = words.as_deref().unwrap_or_else(|| refusal(asks));
            json!({"behavior": "deny", "message": message, "interrupt": interrupt})
        }
        Decision::KeepPlanning { words } => {
            json!({"behavior": "deny", "message": words.as_deref().unwrap_or(KEEP_PLANNING), "interrupt": false})
        }
        Decision::Answer { answers } => {
            let Some(Asks::Questions { questions }) = asks else { return None };
            let said = asking::by_text(questions, answers);
            // None answered is how the person chooses not to answer, which the agent is told in so many words.
            if said.is_empty() {
                json!({"behavior": "deny", "message": NOT_ANSWERED, "interrupt": false})
            } else {
                answered(input(), &said)
            }
        }
        Decision::Deny { .. }
        | Decision::ApprovePlan { .. }
        | Decision::Grant { .. }
        | Decision::Respond(_)
        | Decision::Unknown => return None,
    })
}

/// The CLI takes answers as part of the tool's input.
fn answered(mut input: Value, answers: &BTreeMap<String, String>) -> Value {
    input["answers"] = json!(answers);
    json!({"behavior": "allow", "updatedInput": input})
}

/// The CLI's update that keeps `rule`, which allows, from its own form, `Tool(content)` or `Tool`, as the request
/// suggested it.
fn kept(rule: &Rule) -> Option<Value> {
    if rule.effect != RuleEffect::Allow {
        return None;
    }
    let (tool, content) = match rule.matching.strip_suffix(')').and_then(|rest| rest.split_once('(')) {
        Some((tool, content)) => (tool, Some(content)),
        None => (rule.matching.as_str(), None),
    };
    let mut kept = json!({"toolName": tool});
    if let Some(content) = content {
        kept["ruleContent"] = content.into();
    }
    Some(json!({"type": "addRules", "rules": [kept], "behavior": "allow", "destination": destination(rule.kept)?}))
}

/// What the agent is told when the person says no and gives no words of their own, as fits what was asked.
fn refusal(asks: Option<&Asks>) -> &'static str {
    match asks {
        Some(Asks::Plan { .. }) => KEEP_PLANNING,
        Some(
            Asks::Call { .. }
            | Asks::Network { .. }
            | Asks::Access { .. }
            | Asks::Questions { .. }
            | Asks::Server(_)
            | Asks::Unknown,
        )
        | None => DENIED,
    }
}
