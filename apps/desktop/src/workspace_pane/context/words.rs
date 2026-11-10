//! What the context's card says in words and numbers.

use protocol::model::{AgentKind, LimitWindow};

/// A count of tokens, to a tenth of a thousand or a million: `950`, `313.5k`, `1.0M`.
#[expect(clippy::cast_precision_loss, reason = "shown to one decimal place, which a count in the millions keeps")]
pub(super) fn tokens(count: u64) -> String {
    match count {
        ..1_000 => count.to_string(),
        // Up to where a tenth of a thousand would round to a thousand thousands.
        1_000..999_950 => format!("{:.1}k", count as f64 / 1e3),
        _ => format!("{:.1}M", count as f64 / 1e6),
    }
}

/// A share from 0 to 1, as a percentage to a tenth: `59.7%`.
pub(super) fn percent(share: f32) -> String {
    format!("{:.1}%", share * 100.)
}

/// What a limit is called. `None` for a window this build has no name for, which the card leaves out.
pub(super) fn window(window: LimitWindow) -> Option<&'static str> {
    match window {
        LimitWindow::FiveHours => Some("5h limit"),
        LimitWindow::SevenDays => Some("7d limit"),
        LimitWindow::Unknown => None,
    }
}

/// What the card says of how an account is logged in when nobody is.
pub(super) const NOT_LOGGED_IN: &str = "Not logged in";

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
    fn tokens_are_counted_in_thousands_and_millions_to_a_tenth() {
        assert_eq!(tokens(0), "0");
        assert_eq!(tokens(950), "950");
        assert_eq!(tokens(1_000), "1.0k");
        assert_eq!(tokens(313_500), "313.5k");
        assert_eq!(tokens(500_000), "500.0k");
        assert_eq!(tokens(999_949), "999.9k");
        assert_eq!(tokens(999_950), "1.0M");
        assert_eq!(tokens(1_000_000), "1.0M");
    }

    #[test]
    fn a_share_is_a_percentage_to_a_tenth() {
        assert_eq!(percent(0.597), "59.7%");
        assert_eq!(percent(0.001), "0.1%");
        assert_eq!(percent(1.), "100.0%");
    }

    #[test]
    fn each_window_has_a_name_and_one_from_a_newer_hub_has_none() {
        assert_eq!(window(LimitWindow::FiveHours), Some("5h limit"));
        assert_eq!(window(LimitWindow::SevenDays), Some("7d limit"));
        assert_eq!(window(LimitWindow::Unknown), None);
    }
}
