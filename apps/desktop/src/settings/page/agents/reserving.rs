//! The panel that sets how much of an account's week the person keeps for themselves, opened from the account's `…`
//! menu, under its row, as the panel renewing a login is. It says what reserving does, a switch of how much, and the
//! account's week as its row will draw it with that reserved. What's chosen is the panel's until Save hands it to the
//! hub, whose answer is the account's record, so the row draws the reserve only once the hub has it.

use client_core::refusals;
use gpui_kit::base::ElementExt as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Bounds, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Task, TestSupportExt as _, Window, div,
    px,
};
use look::Typeset as _;
use protocol::model::{Account, AccountId, Reserve};

use super::super::redraw::bring_into_sight;
use super::Drawing;
use super::kept::Kept;

/// What can be reserved, from none to all of the week, which keeps the account out of Automatic.
const CHOICES: [(Option<Reserve>, &str); 5] = [
    (None, "None"),
    (Reserve::new(10), "10%"),
    (Reserve::new(25), "25%"),
    (Reserve::new(50), "50%"),
    (Some(Reserve::ALL), "All"),
];

/// How wide the drawing of the week is, about as wide as the switch above it.
const WEEK_WIDTH: Pixels = px(320.);

/// The panel, for the account it's open for, with what's chosen in it so far.
pub(super) struct Reserving {
    account: AccountId,
    chosen: Option<Reserve>,
    /// The wait for what the hub answers to Save, while it hasn't. It goes with the panel.
    saving: Option<Task<()>>,
    /// Why the hub didn't take it, for the person to read.
    refused: Option<String>,
    /// Whether to bring the panel into sight as it's next laid out, which it is once as it opens.
    reveal: bool,
}

impl Kept {
    /// Opens the panel for `account`, in place of any that was open, with what it reserves now chosen.
    pub(super) fn open_reserving(&mut self, account: &Account, cx: &mut Context<Self>) {
        let (account, chosen) = (account.id.clone(), account.reserve);
        self.reserving = Some(Reserving { account, chosen, saving: None, refused: None, reveal: true });
        cx.notify();
    }

    fn choose_reserve(&mut self, chosen: Option<Reserve>, cx: &mut Context<Self>) {
        if let Some(reserving) = self.reserving.as_mut() {
            reserving.chosen = chosen;
            cx.notify();
        }
    }

    /// Hands the hub what's chosen, and closes the panel once it has taken it, or says why it didn't.
    fn save_reserve(&mut self, cx: &mut Context<Self>) {
        let (Some(asking), Some(reserving)) = (self.asking.as_ref(), self.reserving.as_mut()) else { return };
        if reserving.saving.is_some() {
            return;
        }
        let (account, chosen) = (reserving.account.clone(), reserving.chosen);
        let saving = asking.client.start(move |client| client.account_set_reserve(&account, chosen));
        reserving.refused = None;
        reserving.saving = Some(cx.spawn(async move |this, cx| {
            let answer = saving.recv().await;
            let _ = this.update(cx, |this, cx| {
                // client-core takes in the account's record the reply carries, so the row draws it as the panel goes.
                match answer {
                    Ok(_) => this.reserving = None,
                    Err(e) => {
                        if let Some(reserving) = this.reserving.as_mut() {
                            (reserving.saving, reserving.refused) = (None, Some(refusals::couldnt("save it", &e)));
                        }
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn close_reserving(&mut self, cx: &mut Context<Self>) {
        self.reserving = None;
        cx.notify();
    }

    /// Brings the panel into sight on Settings' `scroll`, once as it opens, now that it's laid out at `bounds`, as the
    /// panel renewing a login is.
    fn reserving_laid_out(&mut self, bounds: Bounds<Pixels>, scroll: &ScrollHandle, window: &mut Window) {
        let Some(reserving) = self.reserving.as_mut().filter(|open| open.reveal) else { return };
        reserving.reveal = false;
        if bring_into_sight(scroll, bounds) {
            window.on_next_frame(|window, _| window.refresh());
        }
    }
}

/// The panel for `account`, if it's the one open.
pub(super) fn panel(drawing: &Drawing<'_>, account: &Account, cx: &App) -> Option<AnyElement> {
    let kept = drawing.kept.read(cx);
    let reserving = kept.reserving.as_ref().filter(|open| open.account == account.id)?;
    let colors = look::colors(cx);
    let name = crate::names::called(account);
    let chosen = reserving.chosen;
    let switch = CHOICES.iter().fold(look::switch(look::SwitchKind::Tracked), |switch, (reserve, words)| {
        let (kept, reserve) = (drawing.kept.clone(), *reserve);
        let id = SharedString::from(format!("reserve-{}", reserve.map_or(0, Reserve::percent)));
        switch.words(id, *words, chosen == reserve, move |_, cx| {
            kept.update(cx, |kept, cx| kept.choose_reserve(reserve, cx));
        })
    });
    let lead = format!(
        "Automatic puts no new chats on {name} once its week is down to what's reserved, so that much is there for \
         you. Chats already on {name} carry on, and say when they're using it."
    );
    // An alert, so assistive apps say it as it comes.
    let refused = reserving.refused.clone().map(|refused| {
        let refused = SharedString::from(refused);
        let said = div().id("reserve-refused").role(Role::Alert).aria_label(refused.clone());
        said.typeset(look::Type::Body).text_color(colors.error).child(refused).test_support()
    });
    let (laid_out, scroll) = (drawing.kept.clone(), drawing.scroll.clone());
    let card = look::card(look::CardKind::InPlace, look::CardWidth::Fills)
        .roomy()
        .heading(look::card_heading(format!("Reserve usage on {name}")))
        .under_heading(div().text_color(colors.text_muted).child(lead))
        .child(
            v_flex()
                .gap(look::Space::Small)
                .child(div().emphasis().child("Of its 7d limit"))
                .child(h_flex().child(switch))
                .child(div().typeset(look::Type::Detail).text_color(colors.text_faint).child(says(chosen))),
        )
        .children(week(account, chosen, drawing.now_ms, cx))
        .children(refused)
        .child(buttons(drawing, reserving.saving.is_some() || !kept.can_ask()));
    let panel = div()
        .id("reserve-panel")
        .relative()
        .child(card)
        .on_prepaint(move |bounds, window, cx| {
            laid_out.update(cx, |kept, _| kept.reserving_laid_out(bounds, &scroll, window));
        })
        .test_support();
    Some(div().pb(look::Space::Large).child(panel).into_any_element())
}

/// What reserving `chosen` comes to, under the switch.
fn says(chosen: Option<Reserve>) -> &'static str {
    match chosen {
        None => "Automatic can use all of its week.",
        Some(Reserve::ALL) => "Automatic never puts a chat on it. Only chats you put on it use it.",
        Some(_) => "Automatic stops putting new chats on it at this much left. You can still put one on it yourself.",
    }
}

/// The account's week at `now_ms` as its row will draw it with `chosen` reserved, and how much is left of it, while
/// an agent under it has heard of it.
fn week(account: &Account, chosen: Option<Reserve>, now_ms: i64, cx: &App) -> Option<Div> {
    let colors = look::colors(cx);
    let week = account.week(now_ms)?;
    let left = week.left_percent();
    let resets = week.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx)).map(|when| format!("Resets {when}"));
    // All of it reserved says so under the switch, and splits nothing off the bar.
    let reserved = chosen.filter(|reserve| *reserve != Reserve::ALL);
    let said = [
        Some(format!("{left}% of its week left")),
        resets,
        reserved.map(|reserve| crate::limits::reserve_said(left, reserve)),
    ];
    let said: Vec<String> = said.into_iter().flatten().collect();
    Some(
        v_flex()
            .gap(look::Space::Small)
            .child(div().w(WEEK_WIDTH).child(crate::limits::bar(left, reserved, crate::limits::BAR_IN_A_CARD, cx)))
            .child(div().typeset(look::Type::Detail).text_color(colors.text_faint).child(said.join(" · "))),
    )
}

/// Cancel, then Save, which is `off` while the hub hasn't answered, and with no hub to ask.
fn buttons(drawing: &Drawing<'_>, off: bool) -> Div {
    let (cancel, save) = (drawing.kept.clone(), drawing.kept.clone());
    h_flex()
        .justify_end()
        .gap(look::Space::Small)
        .child(
            look::button("reserve-cancel", look::ButtonKind::Outlined, look::ControlSize::Regular, "Cancel")
                .on_click(move |_, _, cx| cancel.update(cx, Kept::close_reserving)),
        )
        .child(
            look::button("reserve-save", look::ButtonKind::Main, look::ControlSize::Regular, "Save")
                .off(off)
                .on_click(move |_, _, cx| save.update(cx, Kept::save_reserve)),
        )
}
