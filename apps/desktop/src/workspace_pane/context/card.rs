//! The card the context's ring shows while it's hovered: what the context holds, then how much of the account's
//! limits is left, then whose account it is. It says only what it's been told, so a part nobody has said is left
//! out.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, SharedString,
    Styled as _, TestSupportExt as _, div, px, relative,
};
use protocol::model::{AgentAccount, AgentKind, ContextPart, ContextUsage, SessionAccount, UsageLimit};

use super::words;

const WIDTH: Pixels = px(300.);
/// How tall a bar is.
const BAR: Pixels = px(6.);

/// Raised like the app's other cards. `account` is `None` until the hub has said whose it is, and the card is then
/// only what the context holds.
pub(super) fn card(context: &ContextUsage, agent: AgentKind, account: Option<&SessionAccount>, cx: &App) -> AnyElement {
    let edge = cx.theme().border;
    let limits = account.and_then(|account| limits(&account.limits, agent, cx));
    let who = account.and_then(|account| who(&account.account, cx));
    let rule = || div().my_2().h(px(1.)).bg(edge);
    look::raised(cx)
        .id("context-card")
        .flex()
        .flex_col()
        .w(WIDTH)
        .px_3()
        .py_2p5()
        .text_sm()
        .child(holds(context, cx))
        .children(limits.map(|limits| v_flex().child(rule()).child(limits)))
        .children(who.map(|who| v_flex().child(rule()).child(who)))
        .test_support()
        .into_any_element()
}

/// What's used of how much, as a bar, then each part of the context, largest first, as its share of all the context
/// holds. They don't add up to what's used: some are set aside or free, and some the agent loads only when needed.
fn holds(context: &ContextUsage, cx: &App) -> Div {
    let used = format!("{}/{}", words::tokens(context.used_tokens), words::tokens(context.max_tokens));
    let mut parts: Vec<&ContextPart> = context.parts.iter().collect();
    parts.sort_by_key(|part| std::cmp::Reverse(part.tokens));
    let parts = parts.into_iter().map(|part| share(part.name.clone(), words::percent(context.fraction_of(part)), cx));
    v_flex()
        .gap_1()
        .child(heading("Context", Some(used.into()), cx))
        .child(bar(context.used_fraction(), cx))
        .children(parts)
}

/// How much of each limit is left, as a bar that empties, and when it starts again. `None` when there's none to
/// show, as [`current`] says.
fn limits(heard: &[UsageLimit], agent: AgentKind, cx: &App) -> Option<AnyElement> {
    let current: Vec<AnyElement> =
        current(heard, look::now_ms(cx)).into_iter().filter_map(|limit| self::limit(limit, cx)).collect();
    if current.is_empty() {
        return None;
    }
    let limits = v_flex().id("context-limits").gap_1().child(heading(words::usage_heading(agent), None, cx));
    Some(limits.children(current).test_support().into_any_element())
}

/// The limits that still say something true at `now_ms`. One whose window has ended since it was heard has started
/// again, so how much of it was used is no longer so.
fn current(limits: &[UsageLimit], now_ms: i64) -> Vec<&UsageLimit> {
    limits.iter().filter(|limit| limit.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms)).collect()
}

/// `None` for a limit over a window this build has no name for.
fn limit(limit: &UsageLimit, cx: &App) -> Option<AnyElement> {
    let theme = cx.theme();
    let name = words::window(limit.window)?;
    let left = limit.left_percent();
    let resets = limit.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx));
    let said = h_flex()
        .justify_between()
        .child(div().text_color(theme.muted_foreground).child(name))
        .child(div().font_weight(FontWeight::MEDIUM).child(format!("{left}% left")));
    let resets = resets.map(|when| div().text_xs().text_color(look::faint()).child(format!("Resets {when}")));
    Some(v_flex().gap_1().child(said).child(bar(f32::from(left) / 100., cx)).children(resets).into_any_element())
}

/// Who serves the model, the plan, how it's logged in and as whom. `None` when the agent said none of them.
fn who(account: &AgentAccount, cx: &App) -> Option<AnyElement> {
    let login = if account.logged_in { account.login.clone() } else { Some(words::NOT_LOGGED_IN.to_string()) };
    let said = [
        ("Model provider", account.model_provider.clone()),
        ("Plan", account.plan.clone()),
        ("Login method", login),
        ("Email", account.email.clone()),
    ];
    let lines: Vec<Div> = said.into_iter().filter_map(|(name, said)| Some(answer(name, said?, cx))).collect();
    (!lines.is_empty())
        .then(|| v_flex().id("context-account").gap_1().children(lines).test_support().into_any_element())
}

/// A section's name, in medium weight, and what it comes to, if anything, at the far end.
fn heading(name: &'static str, comes_to: Option<SharedString>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .justify_between()
        .child(div().font_weight(FontWeight::MEDIUM).child(name))
        .children(comes_to.map(|comes_to| div().text_color(muted).child(comes_to)))
}

/// A part of the context and its share, at either end of a line. The name is the agent's, so it's the one cut off
/// if they don't both fit, and the share always shows.
fn share(name: impl Into<SharedString>, share: String, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .justify_between()
        .gap_3()
        .child(div().min_w_0().truncate().text_color(muted).child(name.into()))
        .child(div().flex_none().text_color(look::faint()).child(share))
}

/// Something asked of the account and its answer, at either end of a line. The answer is the long part, such as an
/// email, so it's the one cut off if they don't both fit, and what was asked always shows.
fn answer(asked: &'static str, answer: String, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .justify_between()
        .gap_3()
        .child(div().flex_none().text_color(muted).child(asked))
        .child(div().min_w_0().truncate().text_color(look::faint()).child(answer))
}

/// A bar filled from the left by `share`, from 0 to 1. It's drawn at once, with nothing moving, which the kit's own
/// bar and ring don't do: they glide to a new value.
fn bar(share: f32, cx: &App) -> Div {
    let color = cx.theme().foreground;
    div()
        .w_full()
        .h(BAR)
        .rounded_full()
        .bg(color.opacity(0.12))
        .child(div().h_full().rounded_full().bg(color).w(relative(share.clamp(0., 1.))))
}

#[cfg(test)]
mod tests {
    use protocol::model::LimitWindow;

    use super::*;

    #[test]
    fn a_limit_that_has_started_again_since_it_was_heard_isnt_shown() {
        let limit = |window, resets_at_ms| UsageLimit { window, used_percent: 40, resets_at_ms, heard_at_ms: None };
        let (ended, going, open_ended) = (
            limit(LimitWindow::FiveHours, Some(999)),
            limit(LimitWindow::SevenDays, Some(1_001)),
            limit(LimitWindow::SevenDays, None),
        );

        assert_eq!(current(&[ended, going, open_ended], 1_000), [&going, &open_ended]);
        assert_eq!(current(&[going], 1_001), Vec::<&UsageLimit>::new(), "it starts again the moment its window ends");
    }
}
