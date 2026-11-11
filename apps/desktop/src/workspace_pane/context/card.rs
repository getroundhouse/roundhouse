//! The two cards beside Send. The ring's says how much room is left in the chat's context and what's in it. The
//! bars' says how much of the account's limits is left, and whose account it is. Each says only what it's been
//! told, so a part nobody has said is left out.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement, Pixels, SharedString,
    Stateful, Styled, TestSupportExt as _, div, px, relative,
};
use look::{InRegion as _, Region};
use protocol::model::{Account, AgentKind, ContextPart, ContextPartKind, ContextUsage, UsageLimit};

use super::{Usage, words};

const WIDTH: Pixels = px(300.);
/// How tall a bar is.
const BAR: Pixels = px(6.);

/// Raised like the app's other cards, with a card's room around what it holds.
fn surface(region: Region, cx: &App) -> ObservedElement<Stateful<Div>> {
    look::raised(cx).region(region).flex().flex_col().w(WIDTH).px_3().py_2p5().text_sm()
}

/// The ring's card: the chat's context. `account` is given only when the bars aren't drawn, since this is then the
/// one card there is to say whose account it is.
pub(super) fn context_card(context: &ContextUsage, account: Option<&Account>, cx: &App) -> AnyElement {
    let whose = account.and_then(|account| whose(account, Plan::WithWhose, cx));
    surface(Region::ContextCard, cx).child(room(context, cx)).children(whose).into_any_element()
}

/// The bars' card: the plan beside its heading, how much of each limit shown is left, as a bar that empties, and
/// when it starts again, then whose account it is.
pub(super) fn usage_card(usage: &Usage, agent: AgentKind, cx: &App) -> AnyElement {
    let limits = usage.limits.iter().map(|&(name, limit)| self::limit(name, &limit, cx));
    let plan = usage.account.plan.clone().map(SharedString::from);
    let limits = v_flex().gap_2p5().child(heading(words::usage_heading(agent), plan, cx)).children(limits);
    let whose = whose(&usage.account, Plan::BesideHeading, cx);
    surface(Region::UsageCard, cx).child(limits).children(whose).into_any_element()
}

/// How full the context is, as a bar with the room the agent keeps back split off its end, then how much it can
/// still take before the agent shrinks the conversation, then what's in it, in tokens, largest first.
fn room(context: &ContextUsage, cx: &App) -> Div {
    let theme = cx.theme();
    let kept_back = context.tokens_of(ContextPartKind::Buffer);
    let left = words::room_left(room_left(context), kept_back > 0);
    let used = format!("{} used", words::whole_percent(context.used_fraction()));
    let of = format!("{}/{}", look::tokens(context.used_tokens), look::tokens(context.max_tokens));
    let under = h_flex()
        .justify_between()
        .gap_3()
        .text_xs()
        .child(div().min_w_0().truncate().text_color(theme.muted_foreground).child(left))
        .child(div().flex_none().text_color(look::faint()).child(of));
    let parts: Vec<Div> =
        in_it(context).into_iter().map(|part| share(part.name.clone(), look::tokens(part.tokens), cx)).collect();
    v_flex()
        .gap_1()
        .child(heading("Context", Some(used.into()), cx))
        .child(bar_with_room_kept_back(context.used_fraction(), context.fraction(kept_back), cx))
        .child(under)
        // Left out when the agent named no parts, so the card doesn't end in the room above them.
        .children((!parts.is_empty()).then(|| v_flex().mt_2().gap_1().children(parts)))
}

/// How many more tokens the context can take: what isn't used, less what the agent keeps back. None, rather than
/// less, for an agent that says it has used more than that.
fn room_left(context: &ContextUsage) -> u64 {
    let taken = context.used_tokens.saturating_add(context.tokens_of(ContextPartKind::Buffer));
    context.max_tokens.saturating_sub(taken)
}

/// What's in the context, largest first, in the agent's own words. Room that's free or kept back isn't in it, and
/// neither is what the agent loads only when it needs it. A part of a kind nobody said is, since it may be.
fn in_it(context: &ContextUsage) -> Vec<&ContextPart> {
    let held = |part: &&ContextPart| match part.kind {
        ContextPartKind::Used | ContextPartKind::Unknown => true,
        ContextPartKind::Deferred | ContextPartKind::Buffer | ContextPartKind::Free => false,
    };
    let mut parts: Vec<&ContextPart> = context.parts.iter().filter(held).collect();
    parts.sort_by_key(|part| std::cmp::Reverse(part.tokens));
    parts
}

/// A limit called `name`.
fn limit(name: &'static str, limit: &UsageLimit, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let left = limit.left_percent();
    let resets = limit.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx));
    let said = h_flex()
        .justify_between()
        .child(div().text_color(theme.muted_foreground).child(name))
        .child(div().font_weight(FontWeight::MEDIUM).child(format!("{left}% left")));
    let resets = resets.map(|when| div().text_xs().text_color(look::faint()).child(format!("Resets {when}")));
    v_flex().gap_1().child(said).child(bar(f32::from(left) / 100., cx)).children(resets).into_any_element()
}

/// Whether the plan is said with whose account it is, or is already beside a heading above it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Plan {
    WithWhose,
    BesideHeading,
}

/// Whose account it is, under a rule: the email, then more quietly how it's logged in and who serves the model,
/// after the plan unless it's beside a heading. `None` when the worker said none of them.
fn whose(account: &Account, plan: Plan, cx: &App) -> Option<AnyElement> {
    let theme = cx.theme();
    let who = account.email.clone();
    let plan = account.plan.clone().filter(|_| plan == Plan::WithWhose);
    let how: Vec<String> =
        [plan, account.login.clone(), account.model_provider.clone()].into_iter().flatten().collect();
    if who.is_none() && how.is_empty() {
        return None;
    }
    // Either can be long, such as an email, so each is cut off with an ellipsis rather than wrapped.
    let who = who.map(|who| div().truncate().text_color(theme.muted_foreground).child(who));
    let how = (!how.is_empty()).then(|| div().truncate().text_xs().text_color(look::faint()).child(how.join(" · ")));
    let said = v_flex().id("account").gap_0p5().children(who).children(how).test_support();
    Some(v_flex().child(div().my_2().h(px(1.)).bg(theme.border)).child(said).into_any_element())
}

/// A section's name, in medium weight, and what it comes to, if anything, at the far end.
fn heading(name: &'static str, comes_to: Option<SharedString>, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .justify_between()
        .child(div().font_weight(FontWeight::MEDIUM).child(name))
        .children(comes_to.map(|comes_to| div().text_color(muted).child(comes_to)))
}

/// A part of the context and how big it is, at either end of a line. The name is the agent's, so it's the one cut
/// off if they don't both fit, and how big it is always shows.
fn share(name: impl Into<SharedString>, share: String, cx: &App) -> Div {
    let muted = cx.theme().muted_foreground;
    h_flex()
        .justify_between()
        .gap_3()
        .child(div().min_w_0().truncate().text_color(muted).child(name.into()))
        .child(div().flex_none().text_color(look::faint()).child(share))
}

/// A bar filled from the left by `used`, with `kept_back` of it split off its far end by a gap, so the gap is where
/// the agent shrinks the conversation. Both are shares of the whole, from 0 to 1.
fn bar_with_room_kept_back(used: f32, kept_back: f32, cx: &App) -> Div {
    let color = cx.theme().foreground;
    let usable = (1. - kept_back).clamp(0.01, 1.);
    let filled = div().h_full().rounded_full().bg(color).w(relative((used / usable).clamp(0., 1.)));
    let track = || div().h_full().rounded_full().bg(color.opacity(0.12));
    h_flex()
        .w_full()
        .h(BAR)
        .gap(px(3.))
        .child(track().w(relative(usable)).flex_shrink(1.).child(filled))
        .children((kept_back > 0.).then(|| track().flex_1()))
}

/// A bar filled from the left by `share`, from 0 to 1. Like every bar here it's drawn at once, with nothing moving,
/// which the kit's own bar and ring don't do: they glide to a new value.
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
    use super::*;

    fn context(used_tokens: u64, max_tokens: u64, parts: &[(&str, u64, ContextPartKind)]) -> ContextUsage {
        let parts = parts.iter().map(|&(name, tokens, kind)| ContextPart { name: name.to_string(), tokens, kind });
        ContextUsage { used_tokens, max_tokens, parts: parts.collect() }
    }

    #[test]
    fn the_room_left_is_what_isnt_used_or_kept_back_and_never_less_than_none() {
        let kept_back = [("Autocompact buffer", 33_000, ContextPartKind::Buffer)];

        assert_eq!(room_left(&context(313_500, 500_000, &kept_back)), 153_500);
        assert_eq!(room_left(&context(313_500, 500_000, &[])), 186_500, "an agent that keeps none back");
        assert_eq!(room_left(&context(480_000, 500_000, &kept_back)), 0, "into the room it keeps back");
        assert_eq!(room_left(&context(600_000, 500_000, &[])), 0);
    }

    #[test]
    fn only_whats_in_the_context_is_listed_largest_first() {
        let context = context(
            0,
            0,
            &[
                ("System prompt", 4_000, ContextPartKind::Used),
                ("System tools (deferred)", 19_500, ContextPartKind::Deferred),
                ("Messages", 298_500, ContextPartKind::Used),
                ("Memory", 900, ContextPartKind::Unknown),
                ("Autocompact buffer", 33_000, ContextPartKind::Buffer),
                ("Free space", 153_500, ContextPartKind::Free),
            ],
        );

        let listed: Vec<&str> = in_it(&context).into_iter().map(|part| part.name.as_str()).collect();

        assert_eq!(listed, ["Messages", "System prompt", "Memory"], "a part of a kind nobody said may be in it");
    }
}
