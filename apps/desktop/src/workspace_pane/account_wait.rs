//! The card at the chat's end once it couldn't start, since Automatic found no account with usage to spare: each is
//! down to what it reserves of its week, or out of a limit. The hub dropped what was sent, and its record says why, so
//! the card is there until the chat is on an account. It's amber, as a plan's card is, since it waits on the person,
//! and lists the accounts the chat could go on, each with how much of its week is left and what's reserved, and a
//! button that puts the chat on it, for one with usage left, after which the person sends it again. One out of a limit
//! says when it's back instead.

use client_core::records::Snapshot;
use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, IntoElement as _, ParentElement as _, Pixels, SharedString, Styled as _, WeakEntity,
    div, px,
};
use look::{Region, Typeset as _};
use protocol::model::{Account, Reserve, SessionId};

use super::WorkspacePane;
use super::composer::Hub;

/// How wide an account's name is, so the bars line up from row to row.
const NAME: Pixels = px(104.);
/// How wide the bar of an account's week is.
const BAR: Pixels = px(80.);

/// How an account stands for starting the chat on it.
enum Standing {
    /// It has usage left, of which `left` percent of its week, with `reserve` of it reserved.
    Left { left: u8, reserve: Option<Reserve> },
    /// A limit is used up, until `until`, if an agent heard when it starts again.
    Out { until: Option<i64> },
}

/// Who the buttons ask to put the chat on an account, and whether there's a hub to ask.
struct Starting {
    pane: WeakEntity<WorkspacePane>,
    session: SessionId,
    off: bool,
}

impl WorkspacePane {
    /// The card, while the chat showing needs an account, as its record says.
    pub(super) fn account_wait(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let session = self.session().filter(|session| session.needs_an_account)?;
        let now_ms = look::now_ms(cx);
        let records = self.shown.all();
        let mut standings: Vec<(&Account, String, Standing)> = (records.accounts())
            .filter(|account| account.works_for(session.agent, now_ms))
            .map(|account| (account, crate::names::called(records, account), standing(records, account, now_ms)))
            .collect();
        standings.sort_by_cached_key(|(account, called, _)| (called.to_lowercase(), account.id.clone()));
        let starting = Starting { pane: cx.weak_entity(), session: session.id.clone(), off: self.hub == Hub::Away };
        let card = card(&standings, &starting, cx);
        Some(transcript::in_column(card).flex_none().pb(look::Space::Small).into_any_element())
    }
}

/// The card, with a row for each of `standings`.
fn card(standings: &[(&Account, String, Standing)], starting: &Starting, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let any_left = standings.iter().any(|(_, _, standing)| matches!(standing, Standing::Left { .. }));
    let (title, said) = if any_left {
        (
            "No account has usage to spare",
            "Each account is down to what it reserves of its week, or out of a limit, so your message wasn't sent. \
             Start this chat on one anyway, then send it again."
                .to_string(),
        )
    } else {
        ("Every account is out of usage", out_until(standings, cx))
    };
    let heading = look::card_heading(title).mark(Lucide::CircleAlert);
    let rows = standings.iter().map(|(account, called, standing)| row(account, called, standing, starting, cx));
    look::card(look::CardKind::NeedsYou, look::CardWidth::Fills)
        .region(Region::AccountWait)
        .heading(heading)
        .under_heading(div().text_color(colors.text_muted).child(said))
        .rule()
        .child(v_flex().children(rows))
        .into_any_element()
}

/// What the card says when every account is out of a limit: which is back first, and when, if an agent heard.
fn out_until(standings: &[(&Account, String, Standing)], cx: &App) -> String {
    const OUT: &str = "Nothing can start this chat until a limit resets, so your message wasn't sent.";
    let soonest = standings
        .iter()
        .filter_map(|(account, called, standing)| match standing {
            Standing::Out { until: Some(at_ms) } => Some((*at_ms, *account, called)),
            _ => None,
        })
        .min_by_key(|(at_ms, account, _)| (*at_ms, account.id.clone()));
    let back = soonest.and_then(|(at_ms, _, called)| Some((called.clone(), look::day_and_time(at_ms, cx)?)));
    match back {
        Some((account, when)) => format!("{OUT} {account} is back first, at {when}. Send it again then."),
        None => format!("{OUT} Send it again once one is back."),
    }
}

/// An account's row: what it's `called`, its week as a bar with what's reserved split off, and how much is left, then
/// the button that puts the chat on it, while it has usage left, or when it's back.
fn row(account: &Account, called: &str, standing: &Standing, starting: &Starting, cx: &App) -> Div {
    let colors = look::colors(cx);
    let called = called.to_string();
    let name = div().flex_none().w(NAME).truncate();
    let line = h_flex().h(look::RowSize::Large.height()).items_center().gap(look::Space::Large);
    let tall = crate::limits::BAR_IN_A_CARD;
    match standing {
        Standing::Left { left, reserve } => {
            let bar = crate::limits::bar(*left, *reserve, tall, cx);
            let in_reserve = reserve.is_some_and(|reserve| reserve.reached_at(*left));
            let reserved = reserve.map_or_else(String::new, |reserve| format!(", {}% reserved", reserve.percent()));
            let said = div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(if in_reserve { colors.warning } else { colors.text_muted })
                .child(format!("{left}% left{reserved}"));
            line.child(name.emphasis().child(called.clone()))
                .child(div().flex_none().w(BAR).child(bar))
                .child(said)
                .child(start_on(account, &called, starting))
        }
        Standing::Out { until } => {
            let when = until.and_then(|at_ms| look::day_and_time(at_ms, cx));
            let said = when.map_or_else(|| "Out of usage".to_string(), |when| format!("Out until {when}"));
            line.child(name.text_color(colors.text_muted).child(called))
                .child(div().flex_none().w(BAR).child(crate::limits::bar(0, None, tall, cx)))
                .child(div().flex_1().min_w_0().truncate().text_color(colors.text_faint).child(said))
        }
    }
}

/// The button that puts the chat on `account`, called `called`.
fn start_on(account: &Account, called: &str, starting: &Starting) -> AnyElement {
    let id = SharedString::from(format!("start-on-{}", account.id));
    let (pane, session, account) = (starting.pane.clone(), starting.session.clone(), account.clone());
    look::button(id, look::ButtonKind::Edged, look::ControlSize::Small, format!("Start on {called}"))
        .off(starting.off)
        .on_click(move |_, window, cx| {
            let _ = pane.update(cx, |pane, cx| pane.put_on(session.clone(), &account, window, cx));
        })
        .into_any_element()
}

/// How `account` stands at `now_ms`, as `records`, the copy it's from, reads its limits: out while any limit it has
/// heard of is used up, and otherwise how much of its week is left, all of it if no agent has heard.
fn standing(records: &Snapshot, account: &Account, now_ms: i64) -> Standing {
    let limits = records.account_limits(account);
    if limits.used_up(now_ms).next().is_some() {
        // Back once the last of them has started again.
        return Standing::Out { until: limits.used_up(now_ms).filter_map(|limit| limit.resets_at_ms).max() };
    }
    let left = limits.week(now_ms).map_or(100, protocol::model::UsageLimit::left_percent);
    Standing::Left { left, reserve: account.reserve.filter(|reserve| *reserve != Reserve::ALL) }
}
