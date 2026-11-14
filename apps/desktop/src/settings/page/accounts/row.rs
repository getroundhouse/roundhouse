//! An account's row: what it's called, how its login stands and when it was last used, then what's left of its
//! limits, and at the far end what can be done with it, which only an account whose login the person added has. The
//! panel for renewing its login, or connecting it again, opens under it.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{Icon, IconName, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div, px,
};
use look::{Off as _, Shaped as _, Sizing as _, Typeset as _};
use protocol::model::{Account, AgentKind};

use super::words::{self, Standing};
use super::{Drawing, panel};

/// How wide the limits are, so each row's line up with the others'.
const LIMITS_WIDTH: Pixels = px(240.);
/// How wide what can be done with an account is, so the limits of a row with nothing to do line up too.
const CONTROLS_WIDTH: Pixels = px(176.);
/// How wide a limit's name is beside its bar, and how much is left after it, so the bars line up.
const LIMIT_NAME_WIDTH: Pixels = px(88.);
const LEFT_WIDTH: Pixels = px(64.);
/// How tall a limit's bar is: as small as the bars beside Send, since a row has three.
const BAR: Pixels = px(4.);

/// `account`'s row.
pub(super) fn row(drawing: &Drawing<'_>, account: &Account, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let standing = account.login_agent.map(|_| words::standing(account.held_login.as_ref(), drawing.now_ms, cx));
    let icon = if account.login_agent.is_some() { Lucide::KeyRound } else { Lucide::CircleUser };
    let icon = div().flex_none().pt_0p5().child(Icon::new(icon).size_4().text_color(colors.text_muted));
    let called = words::called(account);
    let dim = standing == Some(Standing::Disconnected);
    let name =
        div().truncate().emphasis().text_color(if dim { colors.text_muted } else { colors.text }).child(called.clone());
    let badge = drawing
        .defaults
        .and_then(|defaults| defaults.iter().find(|choice| choice.account == account.id))
        .and_then(|choice| drawing.agents.of(choice.agent))
        .map(|agent| {
            div()
                .flex_none()
                .px_2()
                .py_0p5()
                .corners(look::Corner::Control)
                .bg(colors.raised)
                .typeset(look::Type::Body)
                .text_color(colors.text_muted)
                .child(format!("Default for {}", agent.name))
        });
    let name = h_flex().gap_2().min_w_0().child(name).children(badge);
    let used = words::last_used(account, drawing.now_ms, cx);
    let used = used.map(|used| div().typeset(look::Type::Body).text_color(colors.text_faint).child(used));
    let who =
        v_flex().flex_1().min_w_0().gap_0p5().child(name).children(stands(drawing, account, standing.as_ref(), cx));
    let line = h_flex()
        .items_center()
        .gap_4()
        .py_3p5()
        .child(h_flex().flex_1().min_w_0().items_start().gap_4().child(icon).child(who.children(used)))
        .child(limits(account, drawing.now_ms, cx))
        .child(controls(drawing, account, &called, standing.as_ref(), cx));
    let kept = drawing.kept.read(cx);
    let not_disconnected = kept.not_disconnected(&account.id);
    let not_disconnected = not_disconnected
        .map(|said| div().pb_3().typeset(look::Type::Body).text_color(colors.error).child(said.to_string()));
    let panel = kept.panel.as_ref().filter(|panel| panel.is_for(&account.id));
    let panel = panel.map(|panel| div().pb_4().child(panel::panel(drawing, panel, cx)));
    v_flex()
        .id(SharedString::from(format!("account-{}", account.id)))
        .border_b_1()
        .border_color(colors.edge)
        .child(line)
        .children(not_disconnected)
        .children(panel)
        .test_support()
        .into_any_element()
}

/// What the account is, under its name: the kind of agent its login is for, for one the person added, its plan and how
/// it's logged in, where its record says, and how its login stands. A login that ends soon, as
/// [`words::warns_from_ms`] says, says so in amber, and one that has ended in red, each with a warning sign, so it's said in words too, as one label, for assistive
/// apps, which can't see the colour.
fn stands(drawing: &Drawing<'_>, account: &Account, standing: Option<&Standing>, cx: &App) -> Option<AnyElement> {
    let colors = look::colors(cx);
    let kind = account.login_agent.and_then(|kind| drawing.agents.of(kind)).map(|agent| agent.name.clone());
    let how = match standing {
        Some(Standing::Held(ends)) => ends.clone(),
        Some(Standing::EndingSoon(said) | Standing::Ended(said)) => Some(said.clone()),
        Some(Standing::Disconnected) => Some("Disconnected".to_string()),
        None => None,
    };
    let parts: Vec<String> = [kind, account.plan.clone(), account.login.clone(), how].into_iter().flatten().collect();
    if parts.is_empty() {
        return None;
    }
    let warns = match standing {
        Some(Standing::EndingSoon(_)) => Some(colors.warning),
        Some(Standing::Ended(_)) => Some(colors.error),
        Some(Standing::Held(_) | Standing::Disconnected) | None => None,
    };
    let sign = warns.map(|_| Icon::new(IconName::TriangleAlert).size_3p5().flex_none());
    let said = parts.join(" · ");
    let line = h_flex()
        .id(SharedString::from(format!("account-stands-{}", account.id)))
        .role(Role::Label)
        .aria_label(said.clone())
        .gap_1p5()
        .typeset(look::Type::Body)
        .text_color(warns.unwrap_or(colors.text_muted))
        .children(sign)
        .child(div().min_w_0().truncate().child(said));
    Some(line.test_support().into_any_element())
}

/// How much of each limit is left, as a bar, for the limits there's something true to say of, as the chat's card of
/// them shows. An account no agent has heard a limit of says so.
fn limits(account: &Account, now_ms: i64, cx: &App) -> Div {
    let colors = look::colors(cx);
    let column =
        v_flex().flex_none().w(LIMITS_WIDTH).gap_1p5().typeset(look::Type::Detail).text_color(colors.text_muted);
    if account.limits.is_empty() {
        return column.child(div().text_color(colors.text_faint).child("No limits heard of yet"));
    }
    column.children(crate::limits::shown(&account.limits, now_ms).into_iter().map(|(name, limit)| {
        let left = limit.left_percent();
        let bar = crate::limits::bar(left, BAR, cx);
        h_flex()
            .gap_2()
            .child(div().flex_none().w(LIMIT_NAME_WIDTH).truncate().child(name))
            .child(div().flex_1().child(bar))
            .child(h_flex().flex_none().w(LEFT_WIDTH).justify_end().child(format!("{left}% left")))
    }))
}

/// What can be done with an account whose login the person added: Renew and Disconnect, in a menu, while the hub holds
/// it, since a login lasts long enough that neither is wanted often, and Connect again once it's been disconnected.
/// Each is off with no hub to ask, and Renew and Connect again while the hub doesn't say what makes a token for its
/// kind.
fn controls(drawing: &Drawing<'_>, account: &Account, called: &str, standing: Option<&Standing>, cx: &App) -> Div {
    let column = h_flex().flex_none().w(CONTROLS_WIDTH).justify_end().gap_1();
    let (Some(agent), Some(standing)) = (account.login_agent, standing) else { return column };
    let makes_a_token = drawing.agents.of(agent).is_some_and(|agent| agent.login.is_some());
    let can_ask = drawing.kept.read(cx).can_ask();
    let id = account.id.as_str();
    let open = open_for(drawing, agent, account);
    if *standing == Standing::Disconnected {
        let again = Button::new(SharedString::from(format!("account-again-{id}")))
            .accessibility_label(format!("Connect {called} again"))
            .outline()
            .sized(look::ControlSize::Regular)
            .label("Connect again")
            .off_while(!can_ask || !makes_a_token)
            .on_click(move |_, window, cx| open(window, cx));
        return column.child(again);
    }
    let (kept, id, called) = (drawing.kept.clone(), account.id.clone(), called.to_string());
    let open = std::rc::Rc::new(open);
    let more = Button::new(SharedString::from(format!("account-more-{id}")))
        .accessibility_label(format!("More for {called}"))
        .ghost()
        .sized_square(look::ControlSize::Regular)
        .icon(IconName::Ellipsis)
        .off_while(!can_ask)
        .dropdown_menu(move |menu: PopupMenu, _, _| {
            let (kept, id, called, open) = (kept.clone(), id.clone(), called.clone(), open.clone());
            let renew =
                PopupMenuItem::new("Renew").disabled(!makes_a_token).on_click(move |_, window, cx| open(window, cx));
            menu.item(renew).item(PopupMenuItem::new("Disconnect").on_click(move |_, window, cx| {
                kept.update(cx, |kept, cx| kept.ask_to_disconnect(id.clone(), &called, window, cx));
            }))
        });
    column.child(more)
}

/// Opens the panel for connecting a login of `agent`'s kind in place of `account`'s own.
fn open_for(drawing: &Drawing<'_>, agent: AgentKind, account: &Account) -> impl Fn(&mut Window, &mut App) + use<> {
    let (kept, account) = (drawing.kept.clone(), account.clone());
    move |window, cx| kept.update(cx, |kept, cx| kept.open_for(agent, &account, window, cx))
}

/// When what the rows of `accounts` say of a time next changes, after `now_ms`: as a login starts to warn of its end,
/// as it ends, as a limit shown starts again, so it's no longer shown, and as the next day starts, at
/// `next_day_at_ms`, for a row that says how many days ago it was last used, or how many days its login has left.
pub(super) fn changes_at_ms<'a>(
    accounts: impl Iterator<Item = &'a Account>,
    now_ms: i64,
    next_day_at_ms: Option<i64>,
) -> Option<i64> {
    let mut at_ms = Vec::new();
    let mut says_a_day = false;
    for account in accounts {
        let held = account.held_login.filter(|held| held.ends_at_ms.is_some_and(|ends_at_ms| ends_at_ms > now_ms));
        says_a_day |= account.used_at_ms.is_some() || held.is_some();
        let ends = held.iter().flat_map(|held| [words::warns_from_ms(held), held.ends_at_ms]).flatten();
        let resets = account.limits.iter().filter_map(|limit| limit.resets_at_ms);
        at_ms.extend(ends.chain(resets).filter(|at_ms| *at_ms > now_ms));
    }
    at_ms.extend(next_day_at_ms.filter(|_| says_a_day));
    at_ms.into_iter().min()
}
