//! How the queue looks: a card above the composer, a line to each prompt, with what it carries after its text.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, Size, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::{Hand as _, InRegion as _, Region};
use protocol::model::QueuedPrompt;
use protocol::short_text::one_line;

use super::Editing;
use crate::workspace_pane::WorkspacePane;

/// About five prompts of a line each.
const QUEUE_MOST_HEIGHT: f32 = 200.;

/// Room for a file's usual name, so a long one doesn't crowd out the prompt's text.
const CARRIED_MOST_WIDTH: f32 = 180.;

/// Where a prompt in the queue has got to.
#[derive(Clone, Copy)]
enum Row {
    /// On the session's record, so it can be changed, taken out or sent now. Sending it while the agent works steers
    /// the agent with it.
    Queued { working: bool },
    /// Queued from here, and not yet on the record.
    Pending,
}

impl WorkspacePane {
    /// The showing chat's queue, oldest first, then what's been queued from here that the hub hasn't answered for yet,
    /// or nothing when that's none. A held queue says so.
    pub(in crate::workspace_pane) fn queue_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.session()?;
        let on_record = |pending: &&super::Pending| {
            !session.queued.iter().any(|queued| queued.client_msg_id == pending.prompt.client_msg_id)
        };
        let pending: Vec<&QueuedPrompt> = (self.queue_pending.iter())
            .filter(|pending| pending.session_id == session.id)
            .filter(on_record)
            .map(|pending| &pending.prompt)
            .collect();
        if session.queued.is_empty() && pending.is_empty() {
            return None;
        }
        let queued = Row::Queued { working: session.status.in_turn() };
        let rows: Vec<AnyElement> = (session.queued.iter().map(|prompt| (prompt, queued)))
            .chain(pending.into_iter().map(|prompt| (prompt, Row::Pending)))
            .map(|(prompt, row)| self.queued_row(prompt, row, cx))
            .collect();
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let heading = if session.queue_held { "Queued, waiting since the agent stopped" } else { "Queued" };
        Some(
            v_flex()
                .region(Region::Queue)
                .mx_3()
                .rounded_t_lg()
                .border_1()
                .border_b_0()
                .border_color(border)
                .bg(theme.background)
                .child(
                    div().px_3().py_1p5().border_b_1().border_color(border).text_xs().text_color(muted).child(heading),
                )
                // Its own height, so a long queue scrolls rather than pushing the composer out of the window.
                .child(div().id("queue-rows").max_h(px(QUEUE_MOST_HEIGHT)).overflow_y_scroll().p_1().children(rows))
                .into_any_element(),
        )
    }

    /// One line of the prompt, and once it's on the record, under the pointer, a lit row with buttons to change it,
    /// take it out, and send it now. A pending prompt is faint, and keeps the same row, so it doesn't move once the hub
    /// has it.
    fn queued_row(&self, prompt: &QueuedPrompt, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let id = &prompt.client_msg_id;
        let editing = self.queue_edit.as_ref().filter(|editing| editing.was.client_msg_id == *id);
        if let Some(editing) = editing {
            return editing_row(editing, cx);
        }
        let group = SharedString::from(format!("queued-{id}"));
        let theme = cx.theme();
        // Muted, so they don't compete with the prompt's text, and the row a step lighter than the card under them.
        let (text, muted, lit) = match row {
            Row::Queued { .. } => (theme.foreground, theme.muted_foreground, theme.muted),
            Row::Pending => (look::faint(), theme.muted_foreground, theme.background),
        };
        h_flex()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .w_full()
            .items_center()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded_md()
            .hover(move |style| style.bg(lit))
            .child(queued_line(prompt, text, muted))
            .child(queued_buttons(prompt, row, group, muted, cx))
            .test_support()
            .into_any_element()
    }
}

/// Hidden rather than left out, so the row keeps its height when they show, and a pending prompt's never show.
fn queued_buttons(
    prompt: &QueuedPrompt,
    row: Row,
    group: SharedString,
    muted: Hsla,
    cx: &mut Context<WorkspacePane>,
) -> impl IntoElement {
    let id = &prompt.client_msg_id;
    let button = |name: &str, icon: IconName, tooltip: &'static str| {
        Button::new(ElementId::Name(format!("{name}-queued-{id}").into()))
            .hand()
            .ghost()
            .xsmall()
            .icon(icon)
            .text_color(muted)
            .tooltip(tooltip)
    };
    let (edit, remove) = (button("edit", IconName::Pencil, "Edit"), button("remove", IconName::Trash, "Remove"));
    let buttons = h_flex().flex_none().gap_0p5().invisible();
    let Row::Queued { working } = row else {
        return buttons.child(edit).child(remove).child(button("send", IconName::ArrowUp, "Send now"));
    };
    let (to_edit, to_remove, to_send) = (prompt.clone(), id.clone(), id.clone());
    let edit = edit.on_click(cx.listener(move |t, _, window, cx| t.edit_queued(&to_edit, window, cx)));
    let remove = remove.on_click(cx.listener(move |t, _, _, cx| t.unqueue(to_remove.clone(), cx)));
    let send = button("send", IconName::ArrowUp, if working { "Steer" } else { "Send now" })
        .on_click(cx.listener(move |t, _, _, cx| t.send_queued_now(to_send.clone(), cx)));
    buttons.group_hover(group, Styled::visible).child(edit).child(remove).child(send)
}

/// The editor in place of the prompt's text, with what it carries still beside it, since a change keeps that.
fn editing_row(editing: &Editing, cx: &mut Context<WorkspacePane>) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    // Down by as much as the editor's text is, so the two share a line.
    let beside = div().pt(Size::Medium.input_py()).children(carried(&editing.was, muted));
    h_flex()
        .id(ElementId::Name(format!("queued-editing-{}", editing.was.client_msg_id).into()))
        .items_start()
        .gap_2()
        .px_1()
        .py_1()
        .on_action(cx.listener(|t, _: &Escape, window, cx| t.cancel_edit(window, cx)))
        .child(div().flex_1().min_w_0().child(Textarea::new(&editing.input).with_size(Size::Medium).appearance(false)))
        .child(beside)
        .into_any_element()
}

/// The prompt on one line, which Edit shows all of, and after it what it carries. A prompt of only files has no line.
fn queued_line(prompt: &QueuedPrompt, text: Hsla, muted: Hsla) -> impl IntoElement {
    let line = one_line(&prompt.text, usize::MAX);
    h_flex()
        .flex_1()
        .min_w_0()
        .gap_2()
        .items_center()
        .when(!line.is_empty(), |row| row.child(div().min_w_0().text_sm().text_color(text).truncate().child(line)))
        .children(carried(prompt, muted))
}

/// What a queued prompt carries, after its text: the file's name, or how many there are. The text gives way first, so
/// it shows even after a long line.
fn carried(prompt: &QueuedPrompt, muted: Hsla) -> Option<AnyElement> {
    let label = match prompt.attachments.as_slice() {
        [] => return None,
        [only] => only.name.clone(),
        several => format!("{} files", several.len()),
    };
    Some(
        h_flex()
            .id(ElementId::Name(format!("queued-files-{}", prompt.client_msg_id).into()))
            .flex_none()
            .max_w(px(CARRIED_MOST_WIDTH))
            .gap_1()
            .items_center()
            .text_sm()
            .text_color(muted)
            .child(Icon::new(IconName::File).xsmall().flex_none())
            .child(div().min_w_0().truncate().child(label))
            .test_support()
            .into_any_element(),
    )
}
