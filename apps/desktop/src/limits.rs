//! What's said of an account's limits wherever they show, beside Send as bars, in the chat's card of them, and on an
//! account's row in Agents in Settings and its card: which of them there's something true to say of, and what each is
//! called, for all of them, the bar that empties as one is used, for the cards and Agents' rows, and a limit as both
//! cards say it.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px, relative,
};
use look::Typeset as _;
use protocol::model::{LimitWindow, UsageLimit};

/// How tall a bar is in a card, a limit's or the context's.
pub(crate) const BAR_IN_A_CARD: Pixels = px(6.);

/// Of `limits`, those there's something true to say of at `now_ms`, each with what it's called, in the order the hub
/// gives them, shortest window first. One whose window has ended since it was heard has started again, so how much of
/// it was used is no longer so, and one over a window this build has no name for, or a model's week that doesn't say
/// whose, couldn't be told from another.
pub(crate) fn shown(limits: &[UsageLimit], now_ms: i64) -> Vec<(SharedString, UsageLimit)> {
    let windows = shown_by_window(limits, now_ms).into_iter();
    windows.map(|(window, limit)| (format!("{window} limit").into(), limit)).collect()
}

/// [`shown`]'s limits, each by its window alone, such as `7d`, where there's room for little more than how much is
/// left of each, as on an account's row in Agents.
pub(crate) fn shown_by_window(limits: &[UsageLimit], now_ms: i64) -> Vec<(SharedString, UsageLimit)> {
    let current = limits.iter().filter(|limit| limit.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms));
    current.filter_map(|limit| Some((window(limit)?.into(), limit.clone()))).collect()
}

/// A limit's window, which it's called by with `limit` after it, such as `7d`, or `7d Fable` for a model's own week, by
/// the label its provider gives the model. `None` for a window this build has no name for, or a model's week that
/// doesn't say whose.
fn window(limit: &UsageLimit) -> Option<String> {
    match limit.window {
        LimitWindow::FiveHours => Some("5h".to_string()),
        LimitWindow::SevenDays => Some("7d".to_string()),
        LimitWindow::ModelSevenDays => limit.model.as_ref().map(|model| format!("7d {model}")),
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

/// A limit called `name`, as a card says it: how much of it is left beside its name, a bar that empties as it's used,
/// and when it starts again, if its agent heard when. A model's week is named by its provider's label for the model,
/// which could be long, so the name is cut off before how much is left is. Assistive apps are told it as one label, as
/// [`said_in_a_card`] says it. It's the card's limit `at` that place in its list, since two can have the same name.
pub(crate) fn in_a_card(at: usize, name: &SharedString, limit: &UsageLimit, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let left = limit.left_percent();
    let resets = limit.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx));
    let said = h_flex()
        .justify_between()
        .gap(look::Space::Medium)
        .child(div().min_w_0().truncate().text_color(colors.text_muted).child(name.clone()))
        .child(div().flex_none().emphasis().child(format!("{left}% left")));
    let resets = resets
        .map(|when| div().typeset(look::Type::Detail).text_color(colors.text_faint).child(format!("Resets {when}")));
    v_flex()
        .id(("limit", at))
        .role(Role::Label)
        .aria_label(said_in_a_card(name, limit, cx))
        .gap(look::Space::XSmall)
        .child(said)
        .child(bar(left, BAR_IN_A_CARD, cx))
        .children(resets)
        .test_support()
        .into_any_element()
}

/// What [`in_a_card`] says of a limit called `name`, in words: `5h limit, 71% left, resets 3:00 PM`.
pub(crate) fn said_in_a_card(name: &str, limit: &UsageLimit, cx: &App) -> String {
    let left = format!("{name}, {}% left", limit.left_percent());
    match limit.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx)) {
        Some(when) => format!("{left}, resets {when}"),
        None => left,
    }
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
        let name = |window, model| {
            let limits = [limit(window, model, None)];
            let named = |shown: Vec<(SharedString, UsageLimit)>| shown.first().map(|(name, _)| name.to_string());
            (named(shown(&limits, 0)), named(shown_by_window(&limits, 0)))
        };
        let both = |name: &str, window: &str| (Some(name.to_string()), Some(window.to_string()));

        assert_eq!(name(LimitWindow::FiveHours, None), both("5h limit", "5h"));
        assert_eq!(name(LimitWindow::SevenDays, None), both("7d limit", "7d"));
        assert_eq!(name(LimitWindow::ModelSevenDays, Some("Fable 5.1")), both("7d Fable 5.1 limit", "7d Fable 5.1"));
        assert_eq!(name(LimitWindow::ModelSevenDays, None), (None, None), "a model's week that doesn't say whose");
        assert_eq!(name(LimitWindow::Unknown, None), (None, None));
    }
}
