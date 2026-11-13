//! Handing work to a subagent: what it's for, what kind of agent does it, and what it was asked. Claude runs most
//! subagents in the background, so the call's result only says it started, and the subagent's own events, which
//! name the call as their parent, are what it did.

use serde_json::Value;

use super::{description, str_field};

/// A call that hands work to a subagent. Every input is optional, so the call is always drawn as one, and the subagent's work,
/// which is left out of its own rows, always shows in it.
pub struct Agent<'a> {
    /// What the agent said the subagent is for.
    description: Option<&'a str>,
    /// What kind of agent it is, such as `Explore`.
    pub kind: Option<&'a str>,
    /// What it was asked.
    pub prompt: Option<&'a str>,
}

/// What a subagent is called when the agent didn't say what it's for, wherever the subagent tab is opened from.
pub const UNTITLED: &str = "Agent";

impl<'a> Agent<'a> {
    pub(super) fn read(input: &'a Value) -> Self {
        Self {
            description: description(input),
            kind: str_field(input, "subagent_type"),
            prompt: str_field(input, "prompt"),
        }
    }

    /// What its line says: what the agent said the subagent is for.
    pub fn title(&self) -> &'a str {
        self.description.unwrap_or(UNTITLED)
    }

    /// What the agent said the subagent is for, if it said, which is the title then.
    pub fn described(&self) -> Option<&'a str> {
        self.description
    }
}

/// Claude's own kind of subagent, which can do anything the agent can, so naming it says nothing.
const DEFAULT_KIND: &str = "general-purpose";

/// The kind of agent to show beside what a subagent is for: any but the default, since only a kind chosen for the
/// work, such as `Explore`, which can't change files, or one a project defines, says what the subagent can do.
pub fn shown_kind(kind: Option<&str>) -> Option<&str> {
    kind.filter(|kind| *kind != DEFAULT_KIND)
}

/// Whether a result only says the subagent started in the background. It's addressed to the agent, and the
/// subagent's report comes later, in its own events.
pub fn only_launched(text: &str) -> bool {
    text.starts_with("Async agent launched")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Agent, only_launched, shown_kind};
    use crate::calls::ToolCall;

    #[test]
    fn a_subagent_is_known_by_what_it_is_for() {
        let input =
            json!({ "description": "Find where signals are checked", "subagent_type": "Explore", "prompt": "Look" });

        let agent = Agent::read(&input);

        assert_eq!(agent.title(), "Find where signals are checked");
        assert_eq!(agent.kind, Some("Explore"));
    }

    #[test]
    fn only_a_kind_other_than_the_default_is_shown() {
        assert_eq!(shown_kind(Some("Explore")), Some("Explore"));
        assert_eq!(shown_kind(Some("code-reviewer")), Some("code-reviewer"), "a project's own");
        assert_eq!(shown_kind(Some("general-purpose")), None);
        assert_eq!(shown_kind(None), None);
    }

    #[test]
    fn a_subagent_with_no_description_says_it_is_one() {
        assert_eq!(Agent::read(&json!({ "prompt": "Look", "description": " " })).title(), "Agent");
        assert_eq!(Agent::read(&json!({ "description": " Look around " })).title(), "Look around");
    }

    #[test]
    fn a_hand_off_is_drawn_as_one_whatever_its_input() {
        for input in [json!({}), json!({ "description": "Look" }), json!({ "prompt": 3 })] {
            assert!(matches!(ToolCall::read(None, "Agent", &input), ToolCall::Agent(_)), "{input}");
        }
    }

    #[test]
    fn a_launch_notice_is_told_apart_from_a_report() {
        assert!(only_launched("Async agent launched successfully. (This tool result is internal metadata)"));
        assert!(!only_launched("Only the train and the depot check for a stop signal."));
    }
}
