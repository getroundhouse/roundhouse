//! An account's row, in the design of every account page's: a mark for an agent's account, what it's called, with
//! Default by the default of its kind, how its login was made, what else there is to say, and a menu, which only an
//! account whose login the person added has, and its card, for what its line has no room for. The panel for renewing
//! its login, or connecting it again, opens under it.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::h_flex;
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use look::Typeset as _;
use protocol::model::{Account, Reserve};

use super::super::account_row::{self, Columns, Detail, Picture, Row, Tone};
use super::words::{self, Says, Standing};
use super::{Drawing, account_menu, card, panel, reserving};

/// How wide a row's middle columns are: how its login was made, such as `Max · Claude account`, and what else there is
/// to say, which is as long as `Ends in 30 days, on Dec 1, 2026` after its warning sign.
const COLUMNS: Columns = Columns { how: px(160.), detail: px(240.) };
/// How wide a limit's bar is, beside its window and how much of it is left.
const BAR_WIDTH: Pixels = px(32.);
/// How tall a limit's bar is: as small as the bars beside Send.
const BAR: Pixels = px(4.);

/// `account`'s row.
pub(super) fn row(drawing: &Drawing<'_>, account: &Account, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let standing = account.login_agent.map(|_| words::standing(account.held_login.as_ref(), drawing.now_ms, cx));
    let called = crate::names::called(account);
    let id = SharedString::from(format!("account-{}", account.id));
    let detail = match words::says(account, standing.as_ref(), drawing.now_ms) {
        Says::Standing(said, tone) => Some(Detail::Said(said, tone)),
        Says::Limits(limits) => Some(Detail::Drawn(limits_left(&id, &limits, cx))),
        Says::NoLimitsYet => Some(Detail::Said(words::NO_LIMITS_YET.to_string(), Tone::Faint)),
        Says::Nothing => None,
    };
    let kept = drawing.kept.read(cx);
    let not_disconnected = kept.not_disconnected(&account.id).map(|said| {
        let line = div().pb(look::Space::Medium).typeset(look::Type::Body).text_color(colors.error);
        line.child(said.to_string()).into_any_element()
    });
    let panel = kept.panel.as_ref().filter(|panel| panel.is_for(&account.id));
    let panel =
        panel.map(|panel| div().pb(look::Space::Large).child(panel::panel(drawing, panel, cx)).into_any_element());
    let shown = Row {
        id,
        picture: Picture::Mark(Lucide::Bot),
        dim: standing == Some(Standing::Disconnected),
        default: drawing.is_default(account),
        how: words::how(account, drawing.agents).map(SharedString::from),
        detail,
        menu: account.login_agent.map(|agent| account_menu::menu(drawing, account, agent, &called, cx)),
        card: card::card(account, standing.as_ref(), drawing.now_ms, cx),
        name: called,
        under: not_disconnected.into_iter().chain(panel).chain(reserving::panel(drawing, account, cx)).collect(),
    };
    account_row::row(shown, COLUMNS, cx)
}

/// How much is left of each of `limits`: its window, a bar that empties as it's used, as the chat's card of them has,
/// and how much is left, said in words too, as one label, for assistive apps, which can't see the bars. Tests find it
/// by the row's `id` and `-says`, as they find what else a row says in words.
fn limits_left(id: &SharedString, limits: &[(SharedString, u8, Option<Reserve>)], cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let muted = colors.text_muted;
    let each = limits.iter().map(|(window, left, reserve)| {
        let bar = crate::limits::bar(*left, *reserve, BAR, cx);
        let in_reserve = reserve.is_some_and(|reserve| reserve.reached_at(*left));
        let left = div().child(format!("{left}%"));
        h_flex()
            .flex_none()
            .gap(look::Space::Small)
            .child(window.clone())
            .child(div().flex_none().w(BAR_WIDTH).child(bar))
            .child(if in_reserve { left.text_color(colors.warning) } else { left })
    });
    h_flex()
        .id(SharedString::from(format!("{id}-says")))
        .role(Role::Label)
        .aria_label(words::limits_left(limits))
        .overflow_hidden()
        .gap(look::Space::Large)
        .text_color(muted)
        .children(each)
        .test_support()
        .into_any_element()
}

/// When what the rows of `accounts` and their cards say of a time next changes, after `now_ms`: as a login starts to
/// warn of its end, as it ends, as a limit shown starts again, so it's no longer shown, and as the next day starts, at
/// `next_day_at_ms`, for a row that says how many days its login has left, or a card that says which day the account
/// was last used, or the day a limit starts again.
pub(super) fn changes_at_ms<'a>(
    accounts: impl Iterator<Item = &'a Account>,
    now_ms: i64,
    next_day_at_ms: Option<i64>,
) -> Option<i64> {
    let mut at_ms = Vec::new();
    let mut says_a_day = false;
    for account in accounts {
        let held = account.held_login.filter(|held| held.ends_at_ms.is_some_and(|ends_at_ms| ends_at_ms > now_ms));
        let resets = account.limits.iter().filter_map(|limit| limit.resets_at_ms).filter(|at_ms| *at_ms > now_ms);
        says_a_day |= account.used_at_ms.is_some() || held.is_some() || resets.clone().next().is_some();
        let ends = held.iter().flat_map(|held| [words::warns_from_ms(held), held.ends_at_ms]).flatten();
        at_ms.extend(ends.chain(resets).filter(|at_ms| *at_ms > now_ms));
    }
    at_ms.extend(next_day_at_ms.filter(|_| says_a_day));
    at_ms.into_iter().min()
}
