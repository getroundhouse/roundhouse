//! What the cards beside Send say in words and numbers.

use protocol::model::{AgentDescription, ContextUsage, LimitWindow, MaxSetBy, UsageLimit};

/// A share from 0 to 1, as a percentage to a tenth: `59.7%`.
pub(super) fn percent(share: f32) -> String {
    format!("{:.1}%", share * 100.)
}

/// A share from 0 to 1, as a whole percentage: `63%`.
pub(super) fn whole_percent(share: f32) -> String {
    format!("{:.0}%", share * 100.)
}

/// How much more a context can take. An agent that keeps room back shrinks the conversation once the rest is full,
/// so for one that `compacts` it's how much until then.
pub(super) fn room_left(tokens: u64, compacts: bool) -> String {
    let left = look::tokens(tokens);
    if compacts { format!("{left} until it compacts") } else { format!("{left} left") }
}

/// What made a context smaller than its model's, when something did, such as `Capped at 500.0k of 1.0M by your
/// settings`. It names what did only when the agent said. Nothing, when the agent says its model set the size, since
/// then the model holding more says something else, such as more than the account may use.
pub(super) fn capped(context: &ContextUsage) -> Option<String> {
    let model = context.model_holds_more()?;
    let by = match context.max_set_by {
        Some(MaxSetBy::Model) => return None,
        Some(MaxSetBy::Settings) => " by your settings",
        Some(MaxSetBy::Environment) => " by an environment variable",
        Some(MaxSetBy::Unknown) | None => "",
    };
    Some(format!("Capped at {} of {}{by}", look::tokens(context.max_tokens), look::tokens(model)))
}

/// What a limit is called, such as `7d limit`, or `7d Fable limit` for a model's own week, by the label its provider
/// gives the model. `None` for a window this build has no name for, or a model's week that doesn't say whose, which
/// the card leaves out.
pub(super) fn limit_name(limit: &UsageLimit) -> Option<String> {
    match limit.window {
        LimitWindow::FiveHours => Some("5h limit".to_string()),
        LimitWindow::SevenDays => Some("7d limit".to_string()),
        LimitWindow::ModelSevenDays => limit.model.as_ref().map(|model| format!("7d {model} limit")),
        LimitWindow::Unknown => None,
    }
}

/// The heading over an account's limits, which are on the use of its agent: what the agent's description says, or
/// with none, only that it's the usage.
pub(super) fn usage_heading(agent: Option<&AgentDescription>) -> &str {
    agent.map_or("Usage", |agent| agent.usage_heading.as_str())
}

#[cfg(test)]
mod tests {
    use protocol::model::ModelLabel;

    use super::*;

    #[test]
    fn a_share_is_a_percentage_to_a_tenth() {
        assert_eq!(percent(0.597), "59.7%");
        assert_eq!(percent(0.001), "0.1%");
        assert_eq!(percent(1.), "100.0%");
    }

    #[test]
    fn a_whole_percentage_is_rounded_to_the_nearest() {
        assert_eq!(whole_percent(0.627), "63%");
        assert_eq!(whole_percent(0.004), "0%");
        assert_eq!(whole_percent(1.), "100%");
    }

    #[test]
    fn room_left_is_until_compacting_only_for_an_agent_that_keeps_room_back() {
        assert_eq!(room_left(153_500, true), "153.5k until it compacts");
        assert_eq!(room_left(186_500, false), "186.5k left");
    }

    #[test]
    fn a_context_smaller_than_its_model_says_so_and_what_made_it_smaller_when_the_agent_said() {
        let context = |model_tokens, max_set_by| ContextUsage {
            used_tokens: 465_200,
            max_tokens: 500_000,
            model_tokens,
            max_set_by,
            parts: Vec::new(),
        };
        let capped_by = |by| capped(&context(Some(1_000_000), by));

        assert_eq!(capped_by(Some(MaxSetBy::Settings)).unwrap(), "Capped at 500.0k of 1.0M by your settings");
        assert_eq!(
            capped_by(Some(MaxSetBy::Environment)).unwrap(),
            "Capped at 500.0k of 1.0M by an environment variable"
        );
        assert_eq!(capped_by(Some(MaxSetBy::Unknown)).unwrap(), "Capped at 500.0k of 1.0M");
        assert_eq!(capped_by(None).unwrap(), "Capped at 500.0k of 1.0M");
        assert_eq!(capped_by(Some(MaxSetBy::Model)), None, "the agent says nothing made it smaller");
        assert_eq!(
            capped(&context(Some(500_000), Some(MaxSetBy::Settings))),
            None,
            "a setting of the model's own size"
        );
        assert_eq!(capped(&context(None, Some(MaxSetBy::Settings))), None, "nobody has said what the model holds");
    }

    #[test]
    fn each_window_has_a_name_a_models_week_is_named_by_its_model_and_one_from_a_newer_hub_has_none() {
        let name = |window, model: Option<&str>| {
            let limit = UsageLimit {
                window,
                model: model.map(ModelLabel::new),
                used_percent: 4,
                resets_at_ms: None,
                heard_at_ms: None,
            };
            limit_name(&limit)
        };

        assert_eq!(name(LimitWindow::FiveHours, None).as_deref(), Some("5h limit"));
        assert_eq!(name(LimitWindow::SevenDays, None).as_deref(), Some("7d limit"));
        assert_eq!(name(LimitWindow::ModelSevenDays, Some("Fable 5.1")).as_deref(), Some("7d Fable 5.1 limit"));
        assert_eq!(name(LimitWindow::ModelSevenDays, None), None, "a model's week that doesn't say whose");
        assert_eq!(name(LimitWindow::Unknown, None), None);
    }
}
