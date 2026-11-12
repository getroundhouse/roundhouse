//! What sits beside Send: a ring for how full the chat's context is, and a bar for each of the account's limits.
//! Each shows a card while the pointer rests on it: what the context holds, or how much of the limits is left and
//! whose account it is.

use std::f32::consts::TAU;

use gpui_kit::base::plot::shape::{Arc, ArcData};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::{
    Anchor, App, Bounds, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, canvas, div, px, relative,
};
use protocol::model::{Account, UsageLimit};

use super::WorkspacePane;
use super::composer::{Hub, Slot, middle};

mod card;
mod words;

/// As wide as a mark beside text, so it sits with the composer's other small things.
const RING: Pixels = px(16.);
/// How thick the ring's line is.
const LINE: f32 = 2.;
/// How long the context's card waits after the pointer leaves the ring, so it can reach the card's button.
const CROSSING: std::time::Duration = std::time::Duration::from_millis(300);
/// How tall a limit's bar is, and how far it is from the next. The same, and on the grid, so one bar or two come to a
/// whole number of pixels either side of the slot's middle.
const LIMIT: Pixels = px(4.);
const BETWEEN_LIMITS: Pixels = px(4.);

/// The account the chat showing runs under, as its record says, with each of its limits there's something true to
/// say of, and what it's called, shortest window first as the hub gives them. Worked out once each time the pane is
/// drawn, so the bars, their card and the ring's card agree on whether there are any.
#[derive(Clone)]
pub(super) struct Usage {
    account: Account,
    limits: Vec<(&'static str, UsageLimit)>,
}

/// Of `limits`, those there's something true to say of at `now_ms`, each with what it's called. One whose window has
/// ended since it was heard has started again, so how much of it was used is no longer so, and one over a window this
/// build has no name for couldn't be told from another.
fn shown(limits: &[UsageLimit], now_ms: i64) -> Vec<(&'static str, UsageLimit)> {
    let current = limits.iter().filter(|limit| limit.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms));
    current.filter_map(|limit| Some((words::window(limit.window)?, *limit))).collect()
}

impl WorkspacePane {
    /// The record of the account the chat showing runs under, and which of its limits are shown. `None` until its
    /// agent has launched, when the hub couldn't tell whose account it is, and from a hub that doesn't keep accounts.
    pub(super) fn usage(&self, cx: &App) -> Option<Usage> {
        let account = self.shown.all().account(self.session()?.account.as_ref()?)?.clone();
        let limits = shown(&account.limits, look::now_ms(cx));
        Some(Usage { account, limits })
    }

    /// The ring for the chat showing, once its agent has said how full its context is. Until then there's nothing
    /// true to draw. It's in muted text's colour however full it is, since amber and red already mean something.
    ///
    /// Its card is made from what the pane holds each time the pane is drawn, so it follows the agent while it
    /// shows. It says whose account it is only when no limits are drawn, since their card says it otherwise.
    pub(super) fn context_ring(&self, usage: Option<&Usage>, cx: &mut Context<Self>) -> Option<Slot> {
        let session = self.session()?;
        let context = session.context.clone()?;
        let account = usage.filter(|usage| usage.limits.is_empty()).map(|usage| usage.account.clone());
        let used = context.used_fraction();
        // The whole slot is the ring's to point at, and what the card lines up with, so the ring's own square is as
        // big as the slot.
        let ring = middle().size(Slot::SIZE).child(div().size(RING).child(ring(used, cx)));
        // There's no asking while the record says the hub would refuse, such as while the agent is on a turn, or with no
        // hub to ask, nor an agent that doesn't compact when asked.
        let (pane, off, compacts) =
            (cx.weak_entity(), session.compaction_waits() || self.hub == Hub::Away, self.compacts());
        // It holds a button, so it waits a moment for the pointer to cross the gap between the ring and the card.
        let hover = card_over("context-hover", ring)
            .close_delay(CROSSING)
            .content(move |_, _, cx| card::context_card(&context, account.as_ref(), &pane, compacts, off, cx));
        // Said in words too, since the ring alone says nothing to someone who can't see it.
        let said = format!("Context {} used", words::percent(used));
        Some(Slot::holding(div().id("context").role(Role::Status).aria_label(said).child(hover).test_support()))
    }

    /// A bar for each of the limits shown, once an agent under the account has heard what's left of them. There are
    /// none for an account without limits. They follow the account's record, which the hub changes whenever any agent
    /// under the account hears of a change to its limits, in whichever workspace. They're bars and not a second ring,
    /// so the two can't be taken for each other, and in the ring's colour for the same reason.
    pub(super) fn usage_bars(&self, usage: Option<&Usage>, cx: &mut Context<Self>) -> Option<Slot> {
        // Only beside a chat, as its agent's limits.
        self.session()?;
        let called = SharedString::from(words::usage_heading(self.agent().as_ref()).to_string());
        let usage = usage.filter(|usage| !usage.limits.is_empty())?.clone();
        let left: Vec<f32> = usage.limits.iter().map(|(_, limit)| f32::from(limit.left_percent()) / 100.).collect();
        let least = left.iter().copied().reduce(f32::min)?;
        let bars = middle().size(Slot::SIZE).child(bars(&left, cx));
        // Said in words too, as the ring is. The least of them is the one that runs out first.
        let said = format!("{called} {} left", words::whole_percent(least));
        let hover =
            card_over("usage-hover", bars).content(move |_, _, cx| card::usage_card(&usage, called.clone(), cx));
        Some(Slot::holding(div().id("usage").role(Role::Status).aria_label(said).child(hover).test_support()))
    }
}

impl WorkspacePane {
    /// Asks the agent in the chat showing to compact its conversation now.
    pub(super) fn compact(&mut self, cx: &mut Context<Self>) {
        if let Some(transcript) = self.transcript() {
            transcript.update(cx, |t, cx| t.compact(None, None, cx));
        }
    }
}

/// A card that opens above `slot` while the pointer rests on it, with its right edge on the slot's, so it covers
/// nothing in the bar.
fn card_over(id: &'static str, slot: Div) -> HoverCard {
    look::hover_card(id).appearance(false).anchor(Anchor::BottomRight).trigger(slot)
}

/// A ring filled clockwise from the top by `used`, from 0 to 1, over a fainter whole ring. It's drawn at once, with
/// nothing moving, since only a working agent moves anything.
fn ring(used: f32, cx: &App) -> impl IntoElement {
    let color = cx.theme().muted_foreground;
    canvas(
        |bounds: Bounds<Pixels>, _: &mut Window, _: &mut App| bounds,
        move |_, bounds, window, _| {
            let outer = bounds.size.width.min(bounds.size.height).as_f32() / 2.;
            let arc = Arc::new().inner_radius(outer - LINE).outer_radius(outer);
            arc.paint(&ArcData::new(&(), 0, 1., 0., TAU), color.opacity(0.25), &bounds, window);
            if used > 0. {
                arc.paint(&ArcData::new(&(), 1, used, 0., used * TAU), color, &bounds, window);
            }
        },
    )
    .size_full()
}

/// A bar for each limit, lying flat one above the next and as wide as the ring, filled from the left by how much of
/// the limit is left, from 0 to 1, over a fainter whole bar. So each is its card's bar made small, and empties as
/// that does. They lie flat because two upright bars beside Send are a pause button.
fn bars(left: &[f32], cx: &App) -> impl IntoElement {
    let color = cx.theme().muted_foreground;
    let bar = move |left: f32| {
        let filled = div().h_full().w(relative(left.clamp(0., 1.))).rounded_full().bg(color);
        div().w(RING).h(LIMIT).rounded_full().bg(color.opacity(0.25)).child(filled)
    };
    div().flex().flex_col().gap(BETWEEN_LIMITS).children(left.iter().copied().map(bar))
}

#[cfg(test)]
mod tests {
    use protocol::model::LimitWindow;

    use super::*;

    #[test]
    fn a_limit_that_has_started_again_since_it_was_heard_or_has_no_name_here_isnt_shown() {
        let limit = |window, resets_at_ms| UsageLimit { window, used_percent: 40, resets_at_ms, heard_at_ms: None };
        let (ended, going, open_ended, unnamed) = (
            limit(LimitWindow::FiveHours, Some(999)),
            limit(LimitWindow::SevenDays, Some(1_001)),
            limit(LimitWindow::SevenDays, None),
            limit(LimitWindow::Unknown, None),
        );

        assert_eq!(shown(&[ended, going, open_ended, unnamed], 1_000), [("7d limit", going), ("7d limit", open_ended)]);
        assert_eq!(shown(&[going], 1_001), [], "it starts again the moment its window ends");
    }
}
