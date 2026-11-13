//! What's said of an account's limits wherever they show, beside Send as bars, in the chat's card of them, and in
//! Accounts in Settings: which of them there's something true to say of, and what each is called, for all three, and
//! the bar that empties as one is used, for the card and Accounts.

use gpui_kit::{App, Div, ParentElement as _, Pixels, SharedString, Styled as _, div, relative};
use protocol::model::{LimitWindow, UsageLimit};

/// Of `limits`, those there's something true to say of at `now_ms`, each with what it's called, in the order the hub
/// gives them, shortest window first. One whose window has ended since it was heard has started again, so how much of
/// it was used is no longer so, and one over a window this build has no name for, or a model's week that doesn't say
/// whose, couldn't be told from another.
pub(crate) fn shown(limits: &[UsageLimit], now_ms: i64) -> Vec<(SharedString, UsageLimit)> {
    let current = limits.iter().filter(|limit| limit.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms));
    current.filter_map(|limit| Some((name(limit)?.into(), limit.clone()))).collect()
}

/// What a limit is called, such as `7d limit`, or `7d Fable limit` for a model's own week, by the label its provider
/// gives the model. `None` for a window this build has no name for, or a model's week that doesn't say whose.
fn name(limit: &UsageLimit) -> Option<String> {
    match limit.window {
        LimitWindow::FiveHours => Some("5h limit".to_string()),
        LimitWindow::SevenDays => Some("7d limit".to_string()),
        LimitWindow::ModelSevenDays => limit.model.as_ref().map(|model| format!("7d {model} limit")),
        LimitWindow::Unknown => None,
    }
}

/// A bar `tall` high, filled from the left by `left_percent`, how much of a limit is left from 0 to 100, so it empties
/// as the limit is used. It's drawn at once, with nothing moving, which the kit's own bar doesn't do: it glides to a
/// new value.
pub(crate) fn bar(left_percent: u8, tall: Pixels, cx: &App) -> Div {
    let (color, left) = (look::colors(cx).text, f32::from(left_percent) / 100.);
    div()
        .w_full()
        .h(tall)
        .rounded_full()
        .bg(color.opacity(0.12))
        .child(div().h_full().rounded_full().bg(color).w(relative(left.clamp(0., 1.))))
}

#[cfg(test)]
mod tests {
    use protocol::model::ModelLabel;

    use super::*;

    fn limit(window: LimitWindow, model: Option<&str>, resets_at_ms: Option<i64>) -> UsageLimit {
        UsageLimit { window, model: model.map(ModelLabel::new), used_percent: 40, resets_at_ms, heard_at_ms: None }
    }

    #[test]
    fn a_limit_that_has_started_again_since_it_was_heard_or_has_no_name_here_isnt_shown() {
        let ended = limit(LimitWindow::FiveHours, None, Some(999));
        let going = limit(LimitWindow::SevenDays, None, Some(1_001));
        let open_ended = limit(LimitWindow::SevenDays, None, None);
        let unnamed = limit(LimitWindow::Unknown, None, None);
        let whose = limit(LimitWindow::ModelSevenDays, None, None);
        let fables = limit(LimitWindow::ModelSevenDays, Some("Fable"), None);
        let all = [ended, going.clone(), open_ended.clone(), unnamed, whose, fables.clone()];
        let names = |shown: Vec<(SharedString, UsageLimit)>| {
            shown.into_iter().map(|(name, limit)| (name.to_string(), limit)).collect::<Vec<_>>()
        };

        assert_eq!(
            names(shown(&all, 1_000)),
            [
                ("7d limit".to_string(), going.clone()),
                ("7d limit".to_string(), open_ended),
                ("7d Fable limit".to_string(), fables)
            ]
        );
        assert_eq!(shown(&[going], 1_001), [], "it starts again the moment its window ends");
    }

    #[test]
    fn each_window_has_a_name_a_models_week_is_named_by_its_model_and_one_from_a_newer_hub_has_none() {
        let name = |window, model| name(&limit(window, model, None));

        assert_eq!(name(LimitWindow::FiveHours, None).as_deref(), Some("5h limit"));
        assert_eq!(name(LimitWindow::SevenDays, None).as_deref(), Some("7d limit"));
        assert_eq!(name(LimitWindow::ModelSevenDays, Some("Fable 5.1")).as_deref(), Some("7d Fable 5.1 limit"));
        assert_eq!(name(LimitWindow::ModelSevenDays, None), None, "a model's week that doesn't say whose");
        assert_eq!(name(LimitWindow::Unknown, None), None);
    }
}
