//! What the cards beside Send say in words and numbers.

use protocol::model::{AgentKind, LimitWindow};

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

/// What a limit is called. `None` for a window this build has no name for, which the card leaves out.
pub(super) fn window(window: LimitWindow) -> Option<&'static str> {
    match window {
        LimitWindow::FiveHours => Some("5h limit"),
        LimitWindow::SevenDays => Some("7d limit"),
        LimitWindow::Unknown => None,
    }
}

/// The heading over an account's limits, which are on the use of its agent.
pub(super) fn usage_heading(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "Claude usage",
        AgentKind::Unknown => "Usage",
    }
}

#[cfg(test)]
mod tests {
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
    fn each_window_has_a_name_and_one_from_a_newer_hub_has_none() {
        assert_eq!(window(LimitWindow::FiveHours), Some("5h limit"));
        assert_eq!(window(LimitWindow::SevenDays), Some("7d limit"));
        assert_eq!(window(LimitWindow::Unknown), None);
    }
}
