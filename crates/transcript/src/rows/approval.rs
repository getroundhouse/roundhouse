//! The card that asks for a tool's approval, or a plan's.

use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, IntoElement, ParentElement, SharedString, Styled,
    WeakEntity, Window, div,
};
use protocol::event::ApprovalDecision;
use protocol::model::{ApprovalId, Seq};
use serde_json::Value;

use super::{row, summarize_input, waiting_card};
use crate::transcript::Transcript;

#[expect(
    clippy::too_many_arguments,
    reason = "each is a separate input to the card, and a struct would only bundle them for this call"
)]
pub(super) fn approval_card(
    seq: Seq,
    approval_id: &ApprovalId,
    tool_name: &str,
    input: &Value,
    description: Option<&str>,
    open: bool,
    this: &WeakEntity<Transcript>,
    cx: &App,
) -> AnyElement {
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    let theme = cx.theme();
    let summary = description.map_or_else(|| summarize_input(input).to_string(), str::to_string);
    let answer = |decision: ApprovalDecision| {
        let (this, approval_id) = (this.clone(), approval_id.clone());
        move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
            let _ = this.update(cx, |t, cx| t.answer_approval(approval_id.clone(), decision.clone(), cx));
        }
    };
    let allow = ApprovalDecision::Allow { updated_input: None, updated_permissions: None };
    let answer_button = |name: &'static str, label: &'static str, decision: ApprovalDecision| {
        Button::new(ElementId::NamedInteger(name.into(), seq)).label(label).on_click(answer(decision))
    };
    let (title, content, buttons): (String, AnyElement, Div) = if tool_name == "ExitPlanMode" {
        let plan = input.get("plan").and_then(Value::as_str).unwrap_or_default().to_string();
        let keep_planning = ApprovalDecision::Deny {
            message: "The user wants to keep planning. Ask them what to change.".to_string(),
            interrupt: false,
        };
        (
            "Plan".to_string(),
            TextView::markdown(ElementId::NamedInteger("plan".into(), seq), SharedString::from(plan))
                .selectable(true)
                .into_any_element(),
            h_flex()
                .gap_2()
                .child(answer_button("approve-plan", "Approve plan", allow).primary())
                .child(answer_button("keep-planning", "Keep planning", keep_planning)),
        )
    } else {
        let deny = ApprovalDecision::Deny { message: "The user denied this.".to_string(), interrupt: false };
        (
            format!("{tool_name} needs approval"),
            div()
                .text_xs()
                .font_family(theme.mono_font_family.clone())
                .text_color(theme.muted_foreground)
                .child(summary)
                .into_any_element(),
            h_flex()
                .gap_2()
                .child(answer_button("allow", "Allow", allow).primary())
                .child(answer_button("deny", "Deny", deny)),
        )
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
