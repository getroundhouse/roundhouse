//! The line a turn ends with.

use std::time::Duration;

use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::{AnyElement, App, IntoElement, ParentElement, Styled, div};
use protocol::event::{EventBody, SessionEvent, TurnOutcome};

use super::{colored_line, empty, muted_line, row, turn_times};
use crate::item_key::ItemKey;

/// How the turn `ended`. A finished one says how long it took and when it ended, with a button that copies what the
/// agent said last, its `result`.
pub(super) fn line(ended: &SessionEvent, cx: &App) -> AnyElement {
    let EventBody::TurnEnded { outcome, result, errors, duration_ms, started_at_ms, .. } = &ended.body else {
        return empty();
    };
    match outcome {
        TurnOutcome::Cancelled => muted_line("Stopped"),
        TurnOutcome::Error => {
            let detail = errors.first().cloned().unwrap_or_else(|| "The turn failed".to_string());
            colored_line(detail, cx.theme().danger)
        }
        TurnOutcome::Success | TurnOutcome::Unknown => {
            finished(ended, took_ms(ended.ts_ms, *started_at_ms, *duration_ms), result.as_deref(), cx)
        }
    }
}

/// How long the turn that ended at `ended_at_ms` took. By the hub's clock where it said when the turn started, which is
/// what the working line counts by, so the time doesn't jump when the turn ends. A turn a hub recorded before it said
/// that has the agent's own count.
fn took_ms(ended_at_ms: i64, started_at_ms: Option<i64>, duration_ms: Option<u64>) -> Option<u64> {
    started_at_ms.and_then(|started| u64::try_from(ended_at_ms.checked_sub(started)?).ok()).or(duration_ms)
}

fn finished(ended: &SessionEvent, took_ms: Option<u64>, result: Option<&str>, cx: &App) -> AnyElement {
    let took = took_ms.map(|ms| turn_times::took(Duration::from_millis(ms)));
    let now = look::local_time(look::now_ms(cx), cx);
    let when = look::local_time(ended.ts_ms, cx).zip(now).map(|(at, now)| turn_times::ended(at, now));
    let said = took.into_iter().chain(when).collect::<Vec<_>>().join(" · ");
    // The button is taller than the text. It's centred on the line without making the line taller, so the line sits
    // where any other muted line would.
    let copy = result.filter(|text| !text.trim().is_empty()).map(|text| {
        div().flex().items_center().h_0().child(
            Clipboard::new(ItemKey::Event(ended.seq, 0).element("copy-reply"))
                .value(text.to_string())
                .tooltip("Copy final message")
                .accessibility_label("Copy final message"),
        )
    });
    row().child(h_flex().gap_1().text_xs().text_color(look::faint()).child(said).children(copy)).into_any_element()
}

#[cfg(test)]
mod tests {
    use super::took_ms;

    #[test]
    fn a_turn_is_timed_by_the_hubs_clock_where_the_hub_said_when_it_started() {
        assert_eq!(took_ms(1_600, Some(100), Some(1_400)), Some(1_500));
        assert_eq!(took_ms(1_600, None, Some(1_400)), Some(1_400), "a hub that didn't say");
        assert_eq!(took_ms(1_600, None, None), None);
        assert_eq!(took_ms(1_600, Some(1_700), Some(1_400)), Some(1_400), "a start after the end is no start");
    }
}
