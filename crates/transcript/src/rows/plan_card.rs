//! The card that asks to approve a plan, with the plan. Anything else the agent waits on the person for is asked in
//! the composer's place, as `questions` says.

use client_core::Approval;
use gpui_kit::assets::IconName;
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, App, ClickEvent, ElementId, IntoElement, ParentElement, Styled, Window};
use protocol::capability::approvals::Asks;
use protocol::event::EventBody;
use protocol::model::Seq;

use super::{Shared, empty, row};
use crate::item_key::ItemKey;
use crate::texts::{TextAt, Written};

/// The card for the request `body`, recorded at `seq`, where it asks to approve a plan, with its buttons while it's
/// `open`. Nothing for any other request, which isn't drawn where it was asked.
pub(super) fn plan_card(seq: Seq, body: &EventBody, open: bool, shared: &Shared) -> AnyElement {
    let (EventBody::ApprovalRequested { approval_id, .. }, Some(approval)) = (body, Approval::of(body)) else {
        return empty();
    };
    let Asks::Plan { plan, .. } = approval.asks() else { return empty() };
    let answer = |approve: bool| {
        let (this, approval_id) = (shared.this.clone(), approval_id.clone());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.answer_plan(approval_id.clone(), approve, cx));
        }
    };
    let answer_button = |name: &'static str, kind, label: &'static str, approve: bool| {
        look::button(ElementId::NamedInteger(name.into(), seq.get()), kind, look::ControlSize::Large, label)
            .on_click(answer(approve))
    };
    // Nothing's found in what asks the person something. It points to no images the hub kept, but its diagrams are
    // drawn.
    let content = shared.texts.draw(
        &shared.selection,
        &Written::unfound(TextAt::only(seq, 0), ItemKey::Event(seq, 0).element("plan"), plan),
        Some(shared.linked.images(seq, &[], |drawn| crate::references::drawing(drawn, &shared.references, seq))),
    );
    let buttons = h_flex()
        .gap(look::Space::Small)
        .child(answer_button("approve-plan", look::ButtonKind::Main, "Approve plan", true))
        .child(answer_button("keep-planning", look::ButtonKind::Edged, "Keep planning", false));
    // Amber while it waits on the person, and a card like the chat's others once it's answered.
    let kind = if open { look::CardKind::NeedsYou } else { look::CardKind::Chat };
    let heading = look::card_heading("Plan").mark(IconName::NotebookPen);
    let heading = if open { heading.end("Waiting for you") } else { heading };
    let card =
        look::card(kind, look::CardWidth::Fills).heading(heading).child(content).when(open, |d| d.child(buttons));
    row().child(card).into_any_element()
}
