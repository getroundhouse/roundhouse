use super::super::around::{Around, Heard};
use super::super::{empty, marked_line, muted_line, row, turn_times};
use super::{cache, card};
use crate::find::{FindLook, Marks, Part};
use crate::item_key::ItemKey;
use card::TurnCard;
use gpui_kit::component::{Icon, h_flex};
use gpui_kit::{
    Anchor, AnyElement, App, InteractiveElement as _, IntoElement, ParentElement, Styled, TestSupportExt as _, div,
};
use look::{IconSizing as _, Typeset as _};
use protocol::event::{EventBody, SessionEvent, TurnOutcome};
use std::time::Duration;

/// How the turn `ended`, with what's `around` it: what the chat's agent is called, what the cache holds and how the
/// reply it ended on was heard, read out loud. A finished one says how long it took and when it ended, after how that
/// reply was heard, if that's more than heard to its end, with a button that copies what the agent said last, its
/// `result`, and resting the pointer on what it says shows the turn's card.
/// The newest one's also says what the cache holds, `cache`, as [`cache`] says. A failed one says what went wrong,
/// with its matches.
pub(in super::super) fn line(ended: &SessionEvent, around: &Around, find: &FindLook, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    let (agent, cache, heard) = (around.agent.clone(), around.cache.as_ref(), around.heard);
    let EventBody::TurnEnded { outcome, result, errors, duration_ms, started_at_ms, models, tokens, .. } = &ended.body
    else {
        return empty();
    };
    match outcome {
        TurnOutcome::Cancelled => muted_line("Stopped", cx),
        TurnOutcome::Error => match errors.first() {
            Some(error) => {
                let marks = find.marks(&(ItemKey::Event(ended.seq, 0), Part::TurnError), error);
                marked_line(error.clone(), &marks, colors.stopped)
            }
            None => marked_line("The turn failed".to_string(), &Marks::default(), colors.stopped),
        },
        TurnOutcome::Success | TurnOutcome::Unknown => {
            let (started_at_ms, took_ms) = timed(ended.ts_ms, *started_at_ms, *duration_ms);
            let card = TurnCard {
                agent,
                models: models.clone(),
                started_at_ms,
                ended_at_ms: ended.ts_ms,
                took: took_ms.map(|ms| turn_times::took(Duration::from_millis(ms))),
                tokens: *tokens,
                cache: cache.map(|cache| cache.cache.clone()),
            };
            let mark = cache.and_then(|cache| cache::on_the_line(cache, cx));
            let ending = Ending { result: result.as_deref(), card, mark, heard };
            finished(ended, ending, cx)
        }
    }
}

/// When the turn that ended at `ended_at_ms` started, and how long it took, for its line and its card alike. A start
/// after the end is no start. The time is by the hub's clock where it said when the turn started, which is what the
/// working line counts by, so the time doesn't jump when the turn ends. A turn whose end was recorded before the hub
/// added that has the agent's own count.
fn timed(ended_at_ms: i64, started_at_ms: Option<i64>, duration_ms: Option<u64>) -> (Option<i64>, Option<u64>) {
    let started_at_ms = started_at_ms.filter(|started| *started <= ended_at_ms);
    let took_ms = started_at_ms.and_then(|started| u64::try_from(ended_at_ms.checked_sub(started)?).ok());
    (started_at_ms, took_ms.or(duration_ms))
}

/// What a finished turn's line says, besides when it ended.
struct Ending<'a, M> {
    /// What the agent said last, which the line's button copies.
    result: Option<&'a str>,
    card: TurnCard,
    /// What says the cache is about to expire or has.
    mark: Option<M>,
    /// How the reply it ended on was heard, read out loud.
    heard: Option<Heard>,
}

/// A finished turn's line, which says how the reply it ended on was heard, if it was read out loud and that's more
/// than heard to its end, then how long the turn took, as the working line counted it, and when it ended, then the
/// mark that says the cache is about to expire or has. Resting the pointer on how long it took, when it ended or the
/// mark shows the turn's card.
fn finished(ended: &SessionEvent, ending: Ending<'_, impl IntoElement>, cx: &App) -> AnyElement {
    let Ending { result, card, mark, heard } = ending;
    let when = look::day_and_time(ended.ts_ms, cx);
    let said = card.took.clone().into_iter().chain(when).collect::<Vec<_>>().join(" · ");
    // It only shows something while the pointer rests on it, so it keeps the arrow. The card opens centred above it,
    // where it covers neither the line nor the composer, and moves only to stay in the window.
    // The mark is part of what the card opens from, since the card says what it means.
    let said = h_flex()
        .id(ItemKey::Event(ended.seq, 0).element("turn-end"))
        .gap(look::Space::XSmall)
        .child(said)
        .children(mark);
    let said = said.test_support();
    let said = look::hover_card(ItemKey::Event(ended.seq, 0).element("turn-card"))
        .anchor(Anchor::BottomCenter)
        .trigger(said)
        .content(move |_, _, cx| card.card(cx));
    // The button is taller than the text. It's centred on the line without making the line taller, so the line sits
    // where any other muted line would.
    let copy = result.filter(|text| !text.trim().is_empty()).map(|text| {
        let copy = look::copy_button(
            ItemKey::Event(ended.seq, 0).element("copy-reply"),
            text.to_string(),
            "Copy final message",
            cx,
        );
        div().flex().items_center().h_0().child(copy)
    });
    let heard = heard.map(|heard| {
        let icon = Icon::new(heard.icon()).sized(look::IconSize::Small);
        h_flex().gap(look::Space::XSmall).child(icon).child(format!("{} ·", heard.words()))
    });
    let line = h_flex()
        .gap(look::Space::XSmall)
        .typeset(look::Type::Detail)
        .text_color(look::colors(cx).text_faint)
        .children(heard)
        .child(said)
        .children(copy);
    row().child(line).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::timed;

    #[test]
    fn a_turn_is_timed_by_the_hubs_clock_where_the_hub_said_when_it_started() {
        assert_eq!(timed(1_600, Some(100), Some(1_400)), (Some(100), Some(1_500)));
        assert_eq!(timed(1_600, Some(1_600), Some(1_400)), (Some(1_600), Some(0)));
        assert_eq!(timed(1_600, None, Some(1_400)), (None, Some(1_400)), "an end that doesn't say");
        assert_eq!(timed(1_600, None, None), (None, None));
    }

    #[test]
    fn a_start_after_the_end_is_no_start_for_the_line_or_the_card() {
        assert_eq!(timed(1_600, Some(1_700), Some(1_400)), (None, Some(1_400)));
    }
}
