//! What sits beside Send: a ring for how full the chat's context is, and a bar for each of the account's limits.
//! Each shows a card while the pointer rests on it: what the context holds, or how much of the limits is left and
//! whose account it is.

use std::f32::consts::TAU;

use gpui_kit::base::plot::shape::{Arc, ArcData};
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
/// How tall a limit's bar is, on the grid, so the bars come to a whole number of pixels either side of the slot's
/// middle.
const LIMIT: Pixels = px(4.);

/// The account the chat showing runs under, as its record says, with each of its limits there's something true to
/// say of, and what it's called, shortest window first as the hub gives them. Worked out once each time the pane is
/// drawn, so the bars, their card and the ring's card agree on whether there are any.
#[derive(Clone)]
pub(super) struct Usage {
    account: Account,
    limits: Vec<(SharedString, UsageLimit)>,
}

/// Of `limits`, those there's something true to say of at `now_ms`, each with what it's called. One whose window has
/// ended since it was heard has started again, so how much of it was used is no longer so, and one over a window this
/// build has no name for, or a model's week that doesn't say whose, couldn't be told from another.
fn shown(limits: &[UsageLimit], now_ms: i64) -> Vec<(SharedString, UsageLimit)> {
    let current = limits.iter().filter(|limit| limit.resets_at_ms.is_none_or(|at_ms| at_ms > now_ms));
    current.filter_map(|limit| Some((words::limit_name(limit)?.into(), limit.clone()))).collect()
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
        let hover = card_over(look::hover_card_with_a_button("context-hover"), ring)
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
        let bars = middle().size(Slot::SIZE).child(bars(&drawn(&left), cx));
        // Said in words too, as the ring is. The least of them is the one that runs out first.
        let said = format!("{called} {} left", words::whole_percent(least));
        let hover = card_over(look::hover_card("usage-hover"), bars)
            .content(move |_, _, cx| card::usage_card(&usage, called.clone(), cx));
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

/// `card`, opening above `slot` while the pointer rests on it, with its right edge on the slot's, so it covers nothing
/// in the bar.
fn card_over(card: HoverCard, slot: Div) -> HoverCard {
    card.anchor(Anchor::BottomRight).trigger(slot)
}

/// A ring filled clockwise from the top by `used`, from 0 to 1, over a fainter whole ring. It's drawn at once, with
/// nothing moving, since only a working agent moves anything.
fn ring(used: f32, cx: &App) -> impl IntoElement {
    let color = look::colors(cx).text_muted;
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
    let color = look::colors(cx).text_muted;
    let bar = move |left: f32| {
        let filled = div().h_full().w(relative(left.clamp(0., 1.))).rounded_full().bg(color);
        div().w(RING).h(LIMIT).rounded_full().bg(color.opacity(0.25)).child(filled)
    };
    div().flex().flex_col().gap(between_limits(left.len())).children(left.iter().copied().map(bar))
}

/// The most bars there's room for in the slot.
const MOST_BARS: usize = 5;

/// Of how much is left of each limit, in the card's order, what's drawn as bars: all of it while there's room, and
/// otherwise the limits with the least left, which run out first, in the same order. An account can have a week of
/// its own for each of several models, and the card lists every limit.
fn drawn(left: &[f32]) -> Vec<f32> {
    let mut least: Vec<usize> = (0..left.len()).collect();
    least.sort_by(|&a, &b| left[a].total_cmp(&left[b]));
    least.truncate(MOST_BARS);
    least.sort_unstable();
    least.into_iter().map(|at| left[at]).collect()
}

/// How far each of `bars` bars is from the next: as far as a bar is tall while they fit in the slot that way, and
/// closer for four or five, so they fit a whole number of pixels either side of the slot's middle.
fn between_limits(bars: usize) -> Pixels {
    match bars {
        0..=3 => LIMIT,
        4 => px(2.),
        _ => px(1.),
    }
}

#[cfg(test)]
mod tests {
    use protocol::model::{LimitWindow, ModelLabel};

    use super::*;

    #[test]
    fn a_limit_that_has_started_again_since_it_was_heard_or_has_no_name_here_isnt_shown() {
        let limit = |window, resets_at_ms| UsageLimit {
            window,
            model: None,
            used_percent: 40,
            resets_at_ms,
            heard_at_ms: None,
        };
        let (ended, going, open_ended, unnamed) = (
            limit(LimitWindow::FiveHours, Some(999)),
            limit(LimitWindow::SevenDays, Some(1_001)),
            limit(LimitWindow::SevenDays, None),
            limit(LimitWindow::Unknown, None),
        );

        let whose = UsageLimit { model: None, ..limit(LimitWindow::ModelSevenDays, None) };
        let fables = UsageLimit { model: Some(ModelLabel::new("Fable")), ..whose.clone() };
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
    fn however_many_limits_there_are_the_bars_fit_in_the_slot_on_whole_pixels_and_show_those_with_least_left() {
        for bars in 1..=u8::try_from(MOST_BARS).unwrap() {
            let between = between_limits(usize::from(bars)).as_f32();
            let tall = f32::from(bars) * LIMIT.as_f32() + f32::from(bars - 1) * between;
            let either_side = (Slot::SIZE.as_f32() - tall) / 2.;
            assert!(either_side >= 0. && either_side.fract() == 0., "{bars} bars are {tall} px tall");
        }

        assert_eq!(drawn(&[0.5, 0.9]), [0.5, 0.9]);
        assert_eq!(drawn(&[0.3, 0.9, 0.1, 0.8, 0.5, 0.2, 0.7]), [0.3, 0.1, 0.5, 0.2, 0.7]);
    }
}
