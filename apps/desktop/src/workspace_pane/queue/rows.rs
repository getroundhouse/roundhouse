//! How the queue looks: a card above the composer, a line to each prompt, with what it carries after its text.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::component::{Icon, Sizable as _, Size, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement as _, Styled, TestSupportExt as _, div, px,
};
use look::Lined as _;
use look::{IconSizing as _, InRegion as _, Region, Shaped as _, Typeset as _};
use protocol::model::{Proposal, QueuedPrompt, RevisionNumber};
use protocol::prompt::PromptFrom;
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
        let pending = self.client.queueing(session);
        if session.queued.is_empty() && pending.is_empty() {
            return None;
        }
        let queued = Row::Queued { working: session.status.in_turn() };
        let rows: Vec<AnyElement> = (session.queued.iter().map(|prompt| (prompt, queued)))
            .chain(pending.iter().map(|prompt| (prompt, Row::Pending)))
            .map(|(prompt, row)| self.queued_row(prompt, row, cx))
            .collect();
        let colors = look::colors(cx);
        let (border, muted) = (colors.edge, colors.text_muted);
        let heading = if session.queue_held { "Queued, waiting since the agent stopped" } else { "Queued" };
        Some(
            v_flex()
                .region(Region::Queue)
                .mx(look::Space::Medium)
                .corners_top(look::Corner::Card)
                .border_1()
                .border_b_0()
                .border_color(border)
                .bg(colors.pane)
                .child(
                    div()
                        .px(look::Space::Medium)
                        .py(look::Space::Small)
                        .line_under(cx)
                        .typeset(look::Type::Detail)
                        .text_color(muted)
                        .child(heading),
                )
                // Its own height, so a long queue scrolls rather than pushing the composer out of the window.
                .child(
                    div()
                        .id("queue-rows")
                        .max_h(px(QUEUE_MOST_HEIGHT))
                        .overflow_y_scroll()
                        .p(look::Space::XSmall)
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    /// One line of the prompt, and once it's on the record, under the pointer, a lit row with buttons to change it,
    /// take it out, and send it now. A pending prompt is faint, and keeps the same row, so it doesn't move once the hub
    /// has it. One the hub queued to tell the agent what was chosen in answer to a proposal is that proposal's line,
    /// and only one the person wrote can be changed.
    fn queued_row(&self, prompt: &QueuedPrompt, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let id = &prompt.client_msg_id;
        // Its record, once it's come, for what the line says.
        let answered = |answers| {
            let session = self.session()?;
            self.shown.all().proposals(&session.id).iter().find(|p| &p.id == answers).cloned()
        };
        let answers = match &prompt.from {
            PromptFrom::ProposalResponse { proposal, revision } => Some((answered(proposal), *revision)),
            PromptFrom::Person | PromptFrom::Unknown => None,
        };
        let editing = self.queue_edit.as_ref().filter(|editing| editing.was.client_msg_id == *id);
        if let Some(editing) = editing {
            return editing_row(editing, cx);
        }
        let group = SharedString::from(format!("queued-{id}"));
        let colors = look::colors(cx);
        // Muted, so they don't compete with the prompt's text, and the row a step lighter than the card under them.
        let (text, muted, lit) = match row {
            Row::Queued { .. } => (colors.text, colors.text_muted, colors.inset),
            Row::Pending => (colors.text_faint, colors.text_muted, colors.pane),
        };
        h_flex()
            .id(ElementId::Name(group.clone()))
            .group(group.clone())
            .w_full()
            .items_center()
            .gap(look::Space::Small)
            .p(look::Space::Small)
            .corners(look::Corner::Control)
            .hover(move |style| style.bg(lit))
            .child(match &answers {
                Some((proposal, revision)) => {
                    answer_line(proposal.as_deref(), *revision, text, muted).into_any_element()
                }
                None => queued_line(prompt, text, muted).into_any_element(),
            })
            .child(queued_buttons(prompt, row, prompt.from.is_person(), group, cx))
            .test_support()
            .into_any_element()
    }
}

/// Hidden rather than left out, so the row keeps its height when they show, and a pending prompt's never show. Edit
/// is there only for a prompt the person wrote.
fn queued_buttons(
    prompt: &QueuedPrompt,
    row: Row,
    written: bool,
    group: SharedString,
    cx: &mut Context<WorkspacePane>,
) -> impl IntoElement {
    let id = &prompt.client_msg_id;
    let button = |name: &str, icon: IconName, tooltip: &'static str| {
        look::icon_button(
            ElementId::Name(format!("{name}-queued-{id}").into()),
            look::ButtonKind::Quiet,
            look::ControlSize::Small,
            icon,
        )
        .tooltip(tooltip)
    };
    let (edit, remove) = (button("edit", IconName::Pencil, "Edit"), button("remove", IconName::Trash, "Remove"));
    let buttons = h_flex().flex_none().gap(look::Space::XSmall).invisible();
    let Row::Queued { working } = row else {
        return buttons.when(written, |b| b.child(edit)).child(remove).child(button(
            "send",
            IconName::ArrowUp,
            "Send now",
        ));
    };
    let (to_edit, to_remove, to_send) = (prompt.clone(), id.clone(), id.clone());
    let edit = edit.on_click(cx.listener(move |t, _, window, cx| t.edit_queued(&to_edit, window, cx)));
    let remove = remove.on_click(cx.listener(move |t, _, _, cx| t.unqueue(to_remove.clone(), cx)));
    let send = button("send", IconName::ArrowUp, if working { "Steer" } else { "Send now" })
        .on_click(cx.listener(move |t, _, _, cx| t.send_queued_now(to_send.clone(), cx)));
    buttons.group_hover(group, Styled::visible).when(written, |b| b.child(edit)).child(remove).child(send)
}

/// What a queued answer to a proposal says: `Answered`, or `Replied` for another round, its title in a chip, and what
/// was said, as its line in the chat says it once it's sent, or that it answers a proposal, until its record has come.
fn answer_line(proposal: Option<&Proposal>, revision: RevisionNumber, text: Hsla, muted: Hsla) -> impl IntoElement {
    let transcript::ProposalResponseLine { verb, title, said } = transcript::proposal_response_line(proposal, revision);
    let title = title.map(|title| look::chip(look::ChipKind::Proposal, look::ChipSize::Regular, title));
    h_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::Small)
        .items_center()
        .typeset(look::Type::Body)
        .child(Icon::new(transcript::PROPOSAL_ICON).sized(look::IconSize::Regular).flex_none().text_color(muted))
        .child(div().flex_none().text_color(text).child(verb))
        .children(title)
        .child(div().min_w_0().truncate().text_color(muted).child(said))
}

/// The editor in place of the prompt's text, with what it carries still beside it, since a change keeps that.
fn editing_row(editing: &Editing, cx: &mut Context<WorkspacePane>) -> AnyElement {
    let muted = look::colors(cx).text_muted;
    // Down by as much as the editor's text is, so the two share a line.
    let beside = div().pt(Size::Medium.input_py()).children(carried(&editing.was, muted));
    h_flex()
        .id(ElementId::Name(format!("queued-editing-{}", editing.was.client_msg_id).into()))
        .items_start()
        .gap(look::Space::Small)
        .px(look::Space::XSmall)
        .py(look::Space::XSmall)
        .on_action(cx.listener(|t, _: &Escape, window, cx| t.cancel_edit(window, cx)))
        .child(div().flex_1().min_w_0().child(Textarea::new(&editing.input).with_size(Size::Medium).appearance(false)))
        .child(beside)
        .into_any_element()
}

/// The prompt on one line, which Edit shows all of, and after it what it carries. A prompt of only files has no line.
fn queued_line(prompt: &QueuedPrompt, text: Hsla, muted: Hsla) -> impl IntoElement {
    let line = one_line(&prompt.prompt.text, usize::MAX);
    h_flex()
        .flex_1()
        .min_w_0()
        .gap(look::Space::Small)
        .items_center()
        .when(!line.is_empty(), |row| {
            row.child(div().min_w_0().typeset(look::Type::Body).text_color(text).truncate().child(line))
        })
        .children(carried(prompt, muted))
}

/// What a queued prompt carries, after its text: the file's name, or how many there are. The text gives way first, so
/// it shows even after a long line.
fn carried(prompt: &QueuedPrompt, muted: Hsla) -> Option<AnyElement> {
    let label = match prompt.prompt.files.as_slice() {
        [] => return None,
        [only] => only.name.clone(),
        several => format!("{} files", several.len()),
    };
    Some(
        h_flex()
            .id(ElementId::Name(format!("queued-files-{}", prompt.client_msg_id).into()))
            .flex_none()
            .max_w(px(CARRIED_MOST_WIDTH))
            .gap(look::Space::XSmall)
            .items_center()
            .typeset(look::Type::Body)
            .text_color(muted)
            .child(Icon::new(IconName::File).sized(look::IconSize::Regular).flex_none())
            .child(div().min_w_0().truncate().child(label))
            .test_support()
            .into_any_element(),
    )
}
