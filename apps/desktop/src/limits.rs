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
use protocol::model::{Account, LimitWindow, Reserve, UsageLimit};

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
/// new value. What the account reserves of the limit is split off its start by a gap, as the context's bar splits off
/// the room its agent keeps back, since it's the last of the limit a chat uses, and once what's left is down to it,
/// what's left is in the warning's colour, since a chat on it is spending what the person kept for themselves.
pub(crate) fn bar(left_percent: u8, reserve: Option<Reserve>, tall: Pixels, cx: &App) -> Div {
    let colors = look::colors(cx);
    let left = f32::from(left_percent.min(100)) / 100.;
    let track = || div().h_full().rounded_full().bg(colors.text.opacity(0.12));
    let filled = |share: f32, fill| div().h_full().rounded_full().bg(fill).w(relative(share.clamp(0., 1.)));
    let Some(reserve) = reserve else {
        return track().w_full().h(tall).child(filled(left, colors.text));
    };
    let fill = if reserve.reached_at(left_percent) { colors.warning } else { colors.text };
    let kept = f32::from(reserve.percent()) / 100.;
    let rest = (kept < 1.).then(|| track().flex_1().child(filled((left - kept) / (1. - kept), fill)));
    h_flex()
        .w_full()
        .h(tall)
        // The drawing's own gap, where what's reserved starts, rather than a space.
        .gap(px(2.))
        .child(track().w(relative(kept)).flex_shrink_0().child(filled(left / kept, fill)))
        .children(rest)
}

/// How much of `account`'s week is left at `now_ms`, as a menu says it beside the account, such as `82% of 7d`, so
/// choosing one account over another can go by it. `None` while no agent under it has heard of its week.
pub(crate) fn week_left(account: &Account, now_ms: i64) -> Option<SharedString> {
    account.week(now_ms).map(|week| format!("{}% of 7d", week.left_percent()).into())
}

/// What `account` reserves of `limit`: only its week has a reserve.
pub(crate) fn reserve_for(account: &Account, limit: &UsageLimit) -> Option<Reserve> {
    account.reserve.filter(|_| limit.window == LimitWindow::SevenDays)
}

/// Whether `limit` is down to what `account` reserves of it.
pub(crate) fn in_reserve(account: &Account, limit: &UsageLimit) -> bool {
    reserve_for(account, limit).is_some_and(|reserve| reserve.reached_at(limit.left_percent()))
}

/// What's said of a limit's reserve once `left_percent` is left, as `25% reserved`, or once that's down to it,
/// `Using the 25% reserved`.
pub(crate) fn reserve_said(left_percent: u8, reserve: Reserve) -> String {
    let percent = reserve.percent();
    if reserve.reached_at(left_percent) {
        format!("Using the {percent}% reserved")
    } else {
        format!("{percent}% reserved")
    }
}

/// A limit called `name`, as a card says it: how much of it is left beside its name, a bar that empties as it's used,
/// and when it starts again, if its agent heard when, then what the account reserves of it, which its bar splits off,
/// and which turns what's left to the warning's colour once it's down to it. A model's week is named by its provider's
/// label for the model, which could be long, so the name is cut off before how much is left is. Assistive apps are told
/// it as one label, as [`said_in_a_card`] says it. It's the card's limit `at` that place in its list, since two can
/// have the same name.
pub(crate) fn in_a_card(
    at: usize,
    name: &SharedString,
    limit: &UsageLimit,
    reserve: Option<Reserve>,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let left = limit.left_percent();
    let in_reserve = reserve.is_some_and(|reserve| reserve.reached_at(left));
    let resets =
        limit.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx)).map(|when| format!("Resets {when}"));
    let under: Vec<String> = resets.into_iter().chain(reserve.map(|reserve| reserve_said(left, reserve))).collect();
    let how_much = div().flex_none().emphasis().child(format!("{left}% left"));
    let said = h_flex()
        .justify_between()
        .gap(look::Space::Medium)
        .child(div().min_w_0().truncate().text_color(colors.text_muted).child(name.clone()))
        .child(if in_reserve { how_much.text_color(colors.warning) } else { how_much });
    let under = (!under.is_empty())
        .then(|| div().typeset(look::Type::Detail).text_color(colors.text_faint).child(under.join(" · ")));
    v_flex()
        .id(("limit", at))
        .role(Role::Label)
        .aria_label(said_in_a_card(name, limit, reserve, cx))
        .gap(look::Space::XSmall)
        .child(said)
        .child(bar(left, reserve, BAR_IN_A_CARD, cx))
        .children(under)
        .test_support()
        .into_any_element()
}

/// What [`in_a_card`] says of a limit called `name`, in words: `5h limit, 71% left, resets 3:00 PM`, then what the
/// account reserves of it, as `25% reserved`.
pub(crate) fn said_in_a_card(name: &str, limit: &UsageLimit, reserve: Option<Reserve>, cx: &App) -> String {
    let left = limit.left_percent();
    let resets =
        limit.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx)).map(|when| format!("resets {when}"));
    let reserved = reserve.map(|reserve| reserve_said(left, reserve).to_lowercase());
    let parts: Vec<String> = [Some(format!("{name}, {left}% left")), resets, reserved].into_iter().flatten().collect();
    parts.join(", ")
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
