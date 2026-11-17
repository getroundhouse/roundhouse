//! An account's card, shown from its row: what the row's one line has no room for. Which day the account was last
//! used, when the login the person added ends, whether or not its row warns of that yet, and how much is left of each
//! of its limits and when each starts again, which the row has room only for the percentages of, and none for while it
//! warns of the login's end.

use gpui_kit::base::ObservedElement;
use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use look::{InRegion as _, Region, Typeset as _};
use protocol::model::{Account, UsageLimit};

use super::super::account_row::Card;
use super::words::{self, Standing};

/// As wide as the chat's card of the account's limits, which says them the same way.
const WIDTH: Pixels = px(300.);

/// What `account`'s card says at `now_ms`, with how its login stands as `standing`, if the person added it. `None`
/// when it would say nothing: for an account nobody has used, whose login has no end the hub knows of, and every one
/// of whose limits heard has started again.
pub(super) fn card(account: &Account, standing: Option<&Standing>, now_ms: i64, cx: &App) -> Option<Card> {
    let ends = standing.and_then(words::ends).map(str::to_string);
    let lines: Vec<String> = [words::last_used(account, now_ms, cx), ends].into_iter().flatten().collect();
    let limits = crate::limits::shown(&account.limits, now_ms);
    let none_heard = account.limits.is_empty();
    if lines.is_empty() && limits.is_empty() && !none_heard {
        return None;
    }
    let limits_said = limits.iter().map(|(name, limit)| crate::limits::said_in_a_card(name, limit, cx));
    let none_said = none_heard.then(|| words::NO_LIMITS_YET.to_string());
    let said: Vec<String> =
        lines.iter().cloned().chain(limits_said).chain(none_said).map(|line| format!("{line}.")).collect();
    Some(Card { said: said.join(" "), shown: Box::new(move |cx| shown(&lines, &limits, none_heard, cx)) })
}

/// Raised like the app's other cards: each of `lines`, then under a rule, each of `limits`, as the chat's card of them
/// says them, or that none has been heard of yet, faintly, while `none_heard`. Each says itself to assistive apps as a
/// label.
fn shown(lines: &[String], limits: &[(SharedString, UsageLimit)], none_heard: bool, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let lines = (!lines.is_empty()).then(|| {
        let each = lines.iter().enumerate().map(|(at, line)| said(("line", at), line.clone()));
        v_flex().gap(look::Space::XSmall).text_color(colors.text_muted).children(each)
    });
    let each = limits.iter().enumerate().map(|(at, (name, limit))| crate::limits::in_a_card(at, name, limit, cx));
    let none = none_heard.then(|| said("no-limits", words::NO_LIMITS_YET.to_string()).text_color(colors.text_faint));
    let any_below = !limits.is_empty() || none_heard;
    let rule = (lines.is_some() && any_below).then(|| div().my(look::Space::Small).h(look::EDGE).bg(colors.edge));
    let below = any_below.then(|| v_flex().gap(look::Space::Medium).children(each).children(none));
    look::raised(cx)
        .region(Region::AccountCard)
        .flex()
        .flex_col()
        .w(WIDTH)
        .p(look::Space::Medium)
        .typeset(look::Type::Body)
        .children(lines)
        .children(rule)
        .children(below)
        .into_any_element()
}

/// A line of the card, known by `id`, which says itself as a label.
fn said(id: impl Into<ElementId>, line: String) -> ObservedElement<Stateful<Div>> {
    div().id(id).role(Role::Label).aria_label(line.clone()).child(line).test_support()
}
