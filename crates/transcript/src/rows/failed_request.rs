//! The card that says one of the agent's requests to its model failed, and why, with a way to try again.

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, IntoElement, ParentElement, Styled, Window, div, px,
};
use look::Hand as _;
use protocol::event::{FailureKind, RequestFailure};
use protocol::model::Seq;

use super::{Shared, card, from_blocks, row};
use crate::find::{FindLook, Part};
use crate::item_key::ItemKey;
use crate::prose::{TextAt, Written};
use crate::transcript::Transcript;

/// How big the card's icon is, beside its title.
const ICON: f32 = 14.0;

/// The card for the failed request recorded at `seq`: what kind of failure it was, when the limit that refused it
/// starts again, for a limit, and the CLI's own words, which say what to do, with their matches. Red, since it's
/// where the agent stopped short, as an approval's card is amber since it waits on the person. With Retry when
/// `retry`, which asks the agent to carry on.
pub(super) fn card(
    seq: Seq,
    failure: &RequestFailure,
    retry: bool,
    shared: &Shared,
    find: &FindLook,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    let danger = theme.danger;
    let resets = failure.resets_at_ms.and_then(|at_ms| look::day_and_time(at_ms, cx));
    let head = h_flex()
        .gap_2()
        .child(Icon::new(IconName::CircleX).flex_none().size(px(ICON)).text_color(danger))
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title(failure.kind)))
        .children(
            resets.map(|when| div().ml_auto().text_xs().text_color(look::faint()).child(format!("Resets {when}"))),
        );
    let id = (ItemKey::Event(seq, 0), Part::FailedRequest);
    let said = from_blocks(&Written::found(TextAt::only(seq, 0), &id, find, &failure.message), None, shared);
    let card = card::tinted(Some(danger), cx)
        .gap_1p5()
        .child(head)
        .child(div().text_sm().text_color(theme.muted_foreground).child(said))
        .when(retry, |d| d.child(h_flex().pt_1().child(retry_button(seq, shared))));
    row().child(card).into_any_element()
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

/// Asks the agent to carry on from where its request failed, with whatever model the chat runs now.
fn retry_button(seq: Seq, shared: &Shared) -> Button {
    let this = shared.this.clone();
    Button::new(ElementId::NamedInteger("retry".into(), seq)).hand().label("Retry").on_click(
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, Transcript::retry);
        },
    )
}
