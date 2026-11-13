//! The card that asks to approve a plan, with the plan. Anything else the agent waits on the person for is asked in
//! the composer's place, as `questions` says.

use client_core::Approval;
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, ClickEvent, Div, ElementId, IntoElement, ParentElement, Styled, Window, div};
use look::{Hand as _, Typeset as _};
use protocol::event::{ApprovalDecision, Asking, EventBody};
use protocol::model::Seq;

use super::{Shared, card, empty, row};
use crate::item_key::ItemKey;
use crate::texts::{TextAt, Written};

/// The card for the request `body`, recorded at `seq`, where it asks to approve a plan, with its buttons while it's
/// `open`. Nothing for any other request, which isn't drawn where it was asked.
pub(super) fn plan_card(seq: Seq, body: &EventBody, open: bool, shared: &Shared, cx: &App) -> AnyElement {
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    let (EventBody::ApprovalRequested { approval_id, .. }, Some(approval)) = (body, Approval::of(body)) else {
        return empty();
    };
    let Asking::Plan { plan } = approval.asking() else { return empty() };
    let answer = |decision: ApprovalDecision| {
        let (this, approval_id) = (shared.this.clone(), approval_id.clone());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.answer_approval(approval_id.clone(), decision.clone(), cx));
        }
    };
    let allow = approval.allowed();
    let answer_button = |name: &'static str, label: &'static str, decision: ApprovalDecision| {
        Button::new(ElementId::NamedInteger(name.into(), seq.get())).hand().label(label).on_click(answer(decision))
    };
    // Nothing's found in what asks the person something.
    let content = shared.texts.draw(
        &shared.selection,
        &Written::unfound(TextAt::only(seq, 0), ItemKey::Event(seq, 0).element("plan"), plan),
        None,
    );
    let buttons = h_flex()
        .gap_2()
        .child(answer_button("approve-plan", "Approve plan", allow).primary())
        .child(answer_button("keep-planning", "Keep planning", approval.refused()));
    row()
        .child(
            waiting_card(open, cx)
                .gap_2()
                .child(div().typeset(look::Type::Body).emphasis().child("Plan"))
                .child(content)
                .when(open, |d| d.child(buttons)),
        )
        .into_any_element()
}

/// A card that waits on the person. While it waits, it has an amber edge and a faint amber wash.
fn waiting_card(open: bool, cx: &App) -> Div {
    card::tinted(open.then(|| look::colors(cx).needs_you), cx)
}
