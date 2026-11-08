//! How a review comment looks in the diff, and the card for writing one.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, Entity, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, WeakEntity, div, px,
};
use protocol::model::DiffComment;

use super::DiffPanel;

fn annotation_frame(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .mx_3()
        .my_2()
        .max_w(px(380.))
        .px_3()
        .py_2p5()
        .gap_2()
        .rounded_md()
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary)
        .text_sm()
}

pub(super) fn draft_card(input: &Entity<TextareaState>, panel: WeakEntity<DiffPanel>, cx: &App) -> AnyElement {
    let (cancel, save) = (panel.clone(), panel);
    annotation_frame(cx)
        .child(Textarea::new(input))
        .child(
            h_flex()
                .gap_2()
                .child(Button::new("save-comment").primary().label("Comment").on_click(move |_, window, cx| {
                    let _ = save.update(cx, |t, cx| t.save_draft(window, cx));
                }))
                .child(Button::new("cancel-comment").label("Cancel").on_click(move |_, _, cx| {
                    let _ = cancel.update(cx, DiffPanel::cancel_draft);
                })),
        )
        .into_any_element()
}

pub(super) fn comment_card(comment: DiffComment, panel: WeakEntity<DiffPanel>, cx: &App) -> AnyElement {
    annotation_frame(cx)
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child("Your comment, for the agent"))
        .child(comment.text.clone())
        .child(
            div()
                .id(ElementId::Name(format!("delete-{}", comment.id).into()))
                .text_xs()
                .cursor_pointer()
                .text_color(look::faint())
                .on_click(move |_, _, cx| {
                    let _ = panel.update(cx, |t, cx| t.delete_comment(comment.id.clone(), cx));
                })
                .child("Delete"),
        )
        .into_any_element()
}
