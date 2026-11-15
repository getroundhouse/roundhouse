//! The card that says the agent's requests to its model failed, and why, with a way to try again. The agent's own
//! failures in a row, the same failure each time, as the hub says of each, are one card: it says how many times, and
//! what started each try.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::{Icon, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement, Styled,
    TestSupportExt as _, Window, div, px,
};
use look::{Hand as _, Shaped as _, Sizing as _, Typeset as _};
use protocol::event::{FailureKind, RequestFailure};
use protocol::model::Seq;

use super::around::{Around, Started, Tries, Try};
use super::line::{self, Line, Part as LinePart};
use super::{Shared, card, from_blocks, own_turn, row};
use crate::find::{FindLook, Part};
use crate::folds::Fold;
use crate::item_key::ItemKey;
use crate::texts::{TextAt, Written};
use crate::transcript::Transcript;

/// How big the card's icon is, beside its title.
const ICON: f32 = 14.0;

/// The card for the failed request recorded at `seq`: what kind of failure it was, how many times in a row, when the
/// limit that refused it starts again, for a limit, and the CLI's own words, which say what to do, with their matches,
/// then what started each try, as `around` reads them. Red, since it's where the agent stopped short, as an approval's
/// card is amber since it waits on the person. With Retry at its bottom right when `around` offers it, which asks the
/// agent to carry on.
pub(super) fn card(
    seq: Seq,
    failure: &RequestFailure,
    around: &Around,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let stopped = colors.stopped;
    let tries = around.failures.card();
    let resets = failure.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx));
    let head = h_flex()
        .gap(look::Space::Small)
        .child(Icon::new(IconName::CircleX).flex_none().size(px(ICON)).text_color(stopped))
        .child(div().typeset(look::Type::Body).emphasis().child(title(failure.kind)))
        .children(tries.and_then(|tries| count(tries.times, cx)))
        .children(resets.map(|when| {
            div().ml_auto().typeset(look::Type::Detail).text_color(colors.text_faint).child(format!("Resets {when}"))
        }));
    let id = (ItemKey::Event(seq, 0), Part::FailedRequest);
    let words = from_blocks(&Written::found(TextAt::only(seq, 0), &id, find, &failure.message), None, shared);
    let words = div().typeset(look::Type::Body).text_color(colors.text_muted).child(words);
    let retry = around.retry.then(|| div().flex_none().child(retry_button(seq, shared)));
    // Retry sits at the bottom right, beside the CLI's words or, when there are any, the lines under them, whose rule
    // runs the card's width over it.
    let body = match tries.filter(|tries| tries.says_each()) {
        Some(tries) => v_flex().gap(look::Space::Small).child(words).child(
            h_flex()
                .items_end()
                .gap(look::Space::Large)
                .pt(look::Space::Small)
                .border_t_1()
                .border_color(stopped.opacity(0.2))
                .child(v_flex().flex_1().min_w_0().children(lines(seq, tries, shared, cx)))
                .children(retry),
        ),
        None => {
            h_flex().items_end().gap(look::Space::Large).child(div().flex_1().min_w_0().child(words)).children(retry)
        }
    };
    let card = card::tinted(Some(stopped), cx).id(ItemKey::Event(seq, 0).element("failed-request"));
    row().child(card.gap(look::Space::Small).child(head).child(body).test_support()).into_any_element()
}

/// What the card is titled, by what would get past the failure, in words that fit any agent.
fn title(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Limit => "Usage limit reached",
        FailureKind::SignIn => "Couldn't sign in",
        FailureKind::Account => "There's a problem with the account",
        FailureKind::Overloaded => "The model's API is overloaded",
        FailureKind::ServerError => "The model's API failed",
        FailureKind::Refused => "The model's API refused the request",
        FailureKind::Unknown => "The request to the model failed",
    }
}

/// How many times in a row the request failed, on a chip beside the title, when it's more than once.
fn count(times: u32, cx: &App) -> Option<AnyElement> {
    let text = how_many(times)?;
    let colors = look::colors(cx);
    let chip = div()
        .flex_none()
        .px(look::Space::Small)
        .corners(look::Corner::Control)
        .border_1()
        .border_color(colors.stopped.opacity(0.3))
        .typeset(look::Type::Detail)
        .text_color(colors.text_muted)
        .child(text);
    Some(chip.into_any_element())
}

/// How many times in a row, in words: twice reads better than a number, and once says nothing, since one card is one
/// failure.
fn how_many(times: u32) -> Option<String> {
    match times {
        0 | 1 => None,
        2 => Some("Twice".to_string()),
        _ => Some(format!("{times} times")),
    }
}

/// The card's lines, under the CLI's words: the fold of the earlier tries, when there are more than it shows, then a
/// line for what started each try it shows, oldest first, as the agent's line for a turn it started by itself says it:
/// what it was, then how it ended, and when.
fn lines(seq: Seq, tries: &Tries, shared: &Shared, cx: &App) -> Vec<AnyElement> {
    let earlier = tries.earlier();
    let fold = (earlier > 0).then(|| fold(seq, tries, earlier, shared, cx));
    let each = tries.shown().iter().flat_map(|one| lines_of(one, cx)).enumerate().map(|(ix, (line, label))| {
        line.still(ItemKey::Event(seq, ix).element("failed-try"), label, &shared.line_text, cx)
    });
    fold.into_iter().chain(each).collect()
}

/// The lines that say what started `one`, with what each says to a screen reader: one for each task a turn the agent
/// started by itself heard had ended, or one for the rest. A subagent's has no Open button, which its call's line has.
fn lines_of(one: &Try, cx: &App) -> Vec<(Line, String)> {
    let at = look::day_and_time(one.at_ms, cx);
    let (icon, title) = match &one.started {
        Started::Tasks(ended) => return ended.iter().map(|ended| own_turn::line(ended, at.as_deref())).collect(),
        Started::Prompt => (IconName::MessageSquare, "Your prompt"),
        Started::Partway => (IconName::Activity, "Partway through the turn"),
        Started::ByItself => (IconName::CornerDownRight, "Picked up by itself"),
    };
    let line = Line::new(icon, title);
    let Some(at) = at else { return vec![(line, title.to_string())] };
    let label = format!("{title}, {at}");
    vec![(line.part(LinePart::Note(at.into())), label)]
}

/// The line that opens and closes the card's earlier tries, `earlier` of them, for the card the failure at `seq` hosts.
fn fold(seq: Seq, tries: &Tries, earlier: usize, shared: &Shared, cx: &App) -> AnyElement {
    let (open, key) = (tries.open, tries.fold.clone());
    let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
    let text = format!("{earlier} earlier");
    let this = shared.this.clone();
    let id = key.element("failed-earlier");
    let toggle = move |cx: &mut App| {
        let _ = this.update(cx, |t, cx| t.set_open(seq, key.clone(), Fold::Earlier, !open, cx));
    };
    line::opening(id, text.clone().into(), open, toggle, cx)
        .text_color(look::colors(cx).text_muted)
        .child(Icon::new(chevron).flex_none().size(px(look::LINE_ICON)))
        .child(div().typeset(look::Type::Body).child(text))
        .test_support()
        .into_any_element()
}

/// Asks the agent to carry on from where its request failed, with whatever model the chat runs now.
fn retry_button(seq: Seq, shared: &Shared) -> Button {
    let this = shared.this.clone();
    Button::new(ElementId::NamedInteger("retry".into(), seq.get()))
        .sized(look::ControlSize::Large)
        .hand()
        .label("Retry")
        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, Transcript::retry);
        })
}

#[cfg(test)]
mod tests {
    use super::how_many;

    #[test]
    fn the_count_says_twice_and_then_a_number() {
        assert_eq!(how_many(1), None, "once, which is any card");
        assert_eq!(how_many(2).as_deref(), Some("Twice"));
        assert_eq!(how_many(6).as_deref(), Some("6 times"));
    }
}
