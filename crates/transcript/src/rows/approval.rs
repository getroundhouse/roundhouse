//! The card that asks for a tool's approval, or a plan's.

use client_core::Approval;
use client_core::calls;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, IntoElement, ParentElement, Styled, Window, div,
};
use look::Hand as _;
use protocol::event::{ApprovalDecision, Asking, EventBody};
use protocol::model::Seq;

use super::{Shared, card, empty, row};
use crate::item_key::ItemKey;
use crate::prose::{TextAt, Written};

/// The card for the request `body`, recorded at `seq`, with its buttons while it's `open`. A plan has its own, and
/// anything else is asked about as a tool's use, by its tool and input.
pub(super) fn approval_card(seq: Seq, body: &EventBody, open: bool, shared: &Shared, cx: &App) -> AnyElement {
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    let (EventBody::ApprovalRequested { approval_id, tool_name, .. }, Some(approval)) = (body, Approval::of(body))
    else {
        return empty();
    };
    let theme = cx.theme();
    let answer = |decision: ApprovalDecision| {
        let (this, approval_id) = (shared.this.clone(), approval_id.clone());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.answer_approval(approval_id.clone(), decision.clone(), cx));
        }
    };
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    let answer_button = |name: &'static str, label: &'static str, decision: ApprovalDecision| {
        Button::new(ElementId::NamedInteger(name.into(), seq)).hand().label(label).on_click(answer(decision))
    };
    let (title, content, buttons): (String, AnyElement, Div) = match approval.asking() {
        Asking::Plan { plan } => (
            "Plan".to_string(),
            // Nothing's found in what asks the person something.
            shared.prose.draw(
                &shared.selection,
                &Written::unfound(TextAt::only(seq, 0), ItemKey::Event(seq, 0).element("plan"), plan),
                None,
            ),
            h_flex()
                .gap_2()
                .child(answer_button("approve-plan", "Approve plan", allow).primary())
                .child(answer_button("keep-planning", "Keep planning", approval.refused())),
        ),
        Asking::ToolUse { .. } | Asking::Questions { .. } | Asking::Unknown => {
            let deny = answer_button("deny", "Deny", approval.refused());
            let summary = calls::asked_about(body);
            (
                format!("{tool_name} needs approval"),
                div()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .children(summary)
                    .into_any_element(),
                h_flex().gap_2().child(answer_button("allow", "Allow", allow).primary()).child(deny),
            )
        }
    };
    row()
        .child(
            waiting_card(open, cx)
                .gap_2()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                .child(content)
                .when(open, |d| d.child(buttons)),
        )
        .into_any_element()
}

/// A card that waits on the person. While it waits, it has an amber edge and a faint amber wash.
fn waiting_card(open: bool, cx: &App) -> Div {
    card::tinted(open.then(|| cx.theme().warning), cx)
}
