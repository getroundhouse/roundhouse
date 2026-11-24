use super::super::WorkspacePane;
use super::super::composer::{Hub, Slot, middle};
use super::moving::Moves;
use super::{card, words};
use client_core::records::Snapshot;
use gpui_kit::base::plot::shape::{Arc, ArcData};
use gpui_kit::{
    Anchor, App, Bounds, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, canvas, div, px, relative,
};
use look::HoverCard;
use protocol::model::{Account, UsageLimit};
use std::f32::consts::TAU;

/// As wide as a mark beside text, so it sits with the composer's other small things.
const RING: Pixels = px(16.);
/// How thick the ring's line is.
const LINE: f32 = 2.;
/// How tall a limit's bar is, on the grid, so the bars come to a whole number of pixels either side of the slot's
/// middle.
const LIMIT: Pixels = px(4.);

/// The account the chat showing runs under, as its record says, with each of its limits there's something true to
/// say of, and what it's called, as [`crate::limits::shown`] says. Worked out once each time the pane is drawn, so the
/// bars, their card and the ring's card agree on whether there are any.
#[derive(Clone)]
pub(in super::super) struct Usage {
    pub(in super::super) account: Account,
    /// The copy of the hub's records the account is from, which says who it is and what's left of its limits.
    pub(in super::super) records: Snapshot,
    pub(super) limits: Vec<(SharedString, UsageLimit)>,
    /// What the menu that moves the chat to another account offers, if there's one to move it to.
    pub(in super::super) moves: Option<Moves>,
    /// Whether Automatic put the chat on the account, as its record says, which its card says.
    pub(super) chosen: Chosen,
}

/// Who put a chat on its account, as far as its card says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Chosen {
    /// Automatic, by how much of its week the account had to spare.
    Automatically,
    /// The person, as a default or a move, or the agent's own login.
    Otherwise,
}

impl WorkspacePane {
    /// The record of the account the chat showing runs under, and which of its limits are shown. `None` until its
    /// agent has launched, when the hub couldn't tell whose account it is, and before the account's record has arrived.
    pub(in super::super) fn usage(&self, cx: &Context<Self>) -> Option<Usage> {
        let session = self.session()?;
        let records = self.shown.all().clone();
        let account = records.account(session.account.as_ref()?)?.clone();
        let limits = crate::limits::shown(records.account_limits(&account).0, look::now_ms(cx));
        let chosen = if session.placed_automatically { Chosen::Automatically } else { Chosen::Otherwise };
        Some(Usage { account, records, limits, moves: self.moves(session, cx), chosen })
    }

    /// The ring for the chat showing, once its agent has said how full its context is. Until then there's nothing
    /// true to draw. It's in muted text's colour however full it is, since amber and red already mean something.
    ///
    /// Its card is made from what the pane holds each time the pane is drawn, so it follows the agent while it
    /// shows. It says whose account it is only when no limits are drawn, since their card says it otherwise.
    pub(in super::super) fn context_ring(&self, usage: Option<&Usage>, cx: &mut Context<Self>) -> Option<Slot> {
        let session = self.session()?;
        let context = self.shown.all().context(session)?.clone();
        let usage = usage.filter(|usage| usage.limits.is_empty()).cloned();
        let used = context.used_fraction();
        // The whole slot is the ring's to point at, and what the card lines up with, so the ring's own square is as
        // big as the slot.
        let ring = middle().size(Slot::SIZE.height()).child(div().size(RING).child(ring(used, cx)));
        // There's no asking while the record says the hub would refuse, such as while the agent is on a turn, or with no
        // hub to ask, nor an agent that doesn't compact when asked.
        let (pane, off, compacts) =
            (cx.weak_entity(), session.compaction_waits() || self.hub == Hub::Away, self.compacts());
        // It holds a button, so it waits a moment for the pointer to cross the gap between the ring and the card.
        let hover = card_over(look::hover_card_with_a_button("context-hover"), ring)
            .content(move |hold, _, cx| card::context_card(&context, usage.as_ref(), hold, &pane, compacts, off, cx));
        // Said in words too, since the ring alone says nothing to someone who can't see it.
        let said = format!("Context {} used", words::percent(used));
        Some(Slot::holding(div().id("context").role(Role::Status).aria_label(said).child(hover).test_support()))
    }

    /// A bar for each of the limits shown, once an agent under the account has heard what's left of them. There are
    /// none for an account without limits. They follow the account's record, which the hub changes whenever any agent
    /// under the account hears of a change to its limits, in whichever workspace. They're bars and not a second ring,
    /// so the two can't be taken for each other, and in the ring's colour for the same reason.
    pub(in super::super) fn usage_bars(&self, usage: Option<&Usage>, cx: &mut Context<Self>) -> Option<Slot> {
        // Only beside a chat, as its agent's limits.
        self.session()?;
        let called = SharedString::from(words::usage_heading(self.agent().as_ref()).to_string());
        let usage = usage.filter(|usage| !usage.limits.is_empty())?.clone();
        let each: Vec<Bar> = usage
            .limits
            .iter()
            .map(|(_, limit)| Bar {
                left: f32::from(limit.left_percent()) / 100.,
                in_reserve: crate::limits::in_reserve(&usage.account, limit),
            })
            .collect();
        let least = each.iter().map(|bar| bar.left).reduce(f32::min)?;
        let bars = middle().size(Slot::SIZE.height()).child(bars(&drawn(&each), cx));
        // Said in words too, as the ring is. The least of them is the one that runs out first.
        let said = format!("{called} {} left", words::whole_percent(least));
        // It holds a button when the chat can be moved to another account, so it waits for the pointer to cross to it.
        let card = if usage.moves.is_some() { look::hover_card_with_a_button } else { look::hover_card };
        let hover = card_over(card("usage-hover"), bars)
            .content(move |hold, _, cx| card::usage_card(&usage, called.clone(), hold, cx));
        Some(Slot::holding(div().id("usage").role(Role::Status).aria_label(said).child(hover).test_support()))
    }
}

impl WorkspacePane {
    /// Asks the agent in the chat showing to compact its conversation now.
    pub(in super::super) fn compact(&mut self, cx: &mut Context<Self>) {
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
fn bars(each: &[Bar], cx: &App) -> impl IntoElement {
    let colors = look::colors(cx);
    let (color, warning) = (colors.text_muted, colors.warning);
    let bar = move |bar: &Bar| {
        let fill = if bar.in_reserve { warning } else { color };
        let filled = div().h_full().w(relative(bar.left.clamp(0., 1.))).rounded_full().bg(fill);
        div().w(RING).h(LIMIT).rounded_full().bg(color.opacity(0.25)).child(filled)
    };
    div().flex().flex_col().gap(between_limits(each.len())).children(each.iter().map(bar))
}

/// A limit's bar beside Send.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bar {
    /// How much of the limit is left, from 0 to 1.
    left: f32,
    /// Whether it's down to what the account reserves of it, which its fill says in the warning's colour.
    in_reserve: bool,
}

/// The most bars there's room for in the slot.
const MOST_BARS: usize = 5;

/// Of how much is left of each limit, in the card's order, what's drawn as bars: all of it while there's room, and
/// otherwise the limits with the least left, which run out first, in the same order. An account can have a week of
/// its own for each of several models, and the card lists every limit.
fn drawn(each: &[Bar]) -> Vec<Bar> {
    let mut least: Vec<usize> = (0..each.len()).collect();
    least.sort_by(|&a, &b| each[a].left.total_cmp(&each[b].left));
    least.truncate(MOST_BARS);
    least.sort_unstable();
    least.into_iter().map(|at| each[at]).collect()
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
    use super::*;

    #[test]
    fn however_many_limits_there_are_the_bars_fit_in_the_slot_on_whole_pixels_and_show_those_with_least_left() {
        for bars in 1..=u8::try_from(MOST_BARS).unwrap() {
            let between = between_limits(usize::from(bars)).as_f32();
            let tall = f32::from(bars) * LIMIT.as_f32() + f32::from(bars - 1) * between;
            let either_side = (Slot::SIZE.height().as_f32() - tall) / 2.;
            assert!(either_side >= 0. && either_side.fract() == 0., "{bars} bars are {tall} px tall");
        }

        let drawn_of = |left: &[f32]| {
            let each: Vec<Bar> = left.iter().map(|&left| Bar { left, in_reserve: left < 0.25 }).collect();
            drawn(&each).into_iter().map(|bar| (bar.left, bar.in_reserve)).collect::<Vec<_>>()
        };
        assert_eq!(drawn_of(&[0.5, 0.9]), [(0.5, false), (0.9, false)]);
        let shown = drawn_of(&[0.3, 0.9, 0.1, 0.8, 0.5, 0.2, 0.7]);
        assert_eq!(shown, [(0.3, false), (0.1, true), (0.5, false), (0.2, true), (0.7, false)], "each with its own");
    }
}
