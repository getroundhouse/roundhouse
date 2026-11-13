//! How a review comment looks in the diff, and the box for writing one. Both sit on the same raised band, which fills
//! the diff from edge to edge, with their text in a column of the same width, so a comment stays where it was written
//! once it's saved.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, ElementId, Entity, IntoElement, ParentElement, Styled, WeakEntity, div, px, rems,
};
use look::{Hand as _, Off as _, Shaped as _, Sizing as _, Typeset as _};
use protocol::model::DiffComment;

use super::FileDiff;

/// Wide enough to write a sentence or two on one line, and no wider, so a long one still reads easily.
const WIDTH: f32 = 720.;

/// The raised band across the diff, and the column in it that holds the comment. Less space goes below than above, as
/// the buttons there carry space of their own, and the eye measures from their labels. It's in the body's font, where
/// the lines around it are in the code's.
fn band(column: Div, cx: &App) -> AnyElement {
    let colors = look::colors(cx);
    div()
        .w_full()
        .px_6()
        .pt(rems(0.75 + 0.625))
        .pb(rems(0.75 + 0.125))
        .bg(colors.raised)
        .typeset(look::Type::Body)
        .child(column.w_full().max_w(px(WIDTH)).gap_3())
        .into_any_element()
}

/// The comment being written, or edited. Its box has a thin copper edge the whole time, rather than a ring that comes
/// and goes with focus, so it's clear where the comment goes.
pub(super) fn draft_card(
    input: &Entity<TextareaState>,
    editing: bool,
    view: WeakEntity<FileDiff>,
    cx: &App,
) -> AnyElement {
    let colors = look::colors(cx);
    let empty = input.read(cx).text().to_string().trim().is_empty();
    let (cancel, save) = (view.clone(), view);
    let field = div()
        .w_full()
        .p_1()
        .corners(look::Corner::Control)
        .border_1()
        .border_color(colors.brand.opacity(0.6))
        .bg(colors.pane)
        .child(Textarea::new(input).appearance(false));
    let actions = h_flex()
        .w_full()
        .justify_end()
        .gap_2()
        .child(Button::new("cancel-comment").sized(look::ControlSize::Regular).hand().ghost().label("Cancel").on_click(
            move |_, window, cx| {
                let _ = cancel.update(cx, |t, cx| t.cancel_draft(window, cx));
            },
        ))
        .child(
            Button::new("save-comment")
                .sized(look::ControlSize::Regular)
                .primary()
                .label(if editing { "Save ↵" } else { "Comment ↵" })
                .off_while(empty)
                .on_click(move |_, window, cx| {
                    let _ = save.update(cx, |t, cx| t.save_draft(window, cx));
                }),
        );
    band(v_flex().child(field).child(actions), cx)
}

/// A comment waiting for the agent, with what can be done with it.
pub(super) fn comment_card(comment: DiffComment, view: WeakEntity<FileDiff>, cx: &App) -> AnyElement {
    let id = comment.id.clone();
    let named = |action: &str| ElementId::Name(format!("{action}-{id}").into());
    let (edit, delete, add) = (view.clone(), view.clone(), view);
    let (to_edit, to_delete, to_add) = (id.clone(), id.clone(), id.clone());
    let actions = h_flex()
        .w_full()
        .justify_end()
        .gap_2()
        .child(
            Button::new(named("edit"))
                .hand()
                .ghost()
                .sized(look::ControlSize::Regular)
                .icon(Lucide::Pencil)
                .label("Edit")
                .on_click(move |_, window, cx| {
                    let _ = edit.update(cx, |t, cx| t.edit_comment(&to_edit, window, cx));
                }),
        )
        .child(
            Button::new(named("delete"))
                .hand()
                .ghost()
                .sized(look::ControlSize::Regular)
                .icon(Lucide::Trash)
                .label("Delete")
                .on_click(move |_, _, cx| {
                    let _ = delete.update(cx, |t, cx| t.delete_comment(to_delete.clone(), cx));
                }),
        )
        // Outlined, as it opens the chat, with the comment in its composer.
        .child(
            Button::new(named("add-to-chat"))
                .hand()
                .outline()
                .sized(look::ControlSize::Regular)
                .icon(Lucide::MessageSquarePlus)
                .label("Add to chat")
                .on_click(move |_, _, cx| {
                    let _ = add.update(cx, |t, cx| t.add_to_chat(&to_add, cx));
                }),
        );
    band(v_flex().child(div().px_1().child(comment.text)).child(actions), cx)
}
