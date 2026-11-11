//! The outline: a short tick at the right of the chat for each prompt, with the one being read in white, so a long chat
//! shows how far it goes and where you are in it. Resting the pointer on the ticks lists the prompts over them, and
//! clicking one jumps to it. The ticks themselves aren't clicked, so they can stay small.

use std::ops::Range;
use std::rc::Rc;

use client_core::OpenSession;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Context, DispatchPhase, Div, ElementId, Entity, EntityId, InteractiveElement,
    IntoElement, ParentElement, Pixels, Role, ScrollEvent, StatefulInteractiveElement, Styled, TestSupportExt as _,
    Window, canvas, deferred, div, px,
};
use look::{InRegion as _, LineText, Region};
use protocol::model::PromptLine;

use super::Transcript;
use crate::list::TranscriptList;
use hover::{Hover, Part, Showing};

mod hover;
mod list;
#[cfg(test)]
mod tests;

/// The most ticks shown at once. A chat with more shows those around the prompt being read, and fades a tick at an
/// end that has more past it.
const MOST_TICKS: usize = 16;
/// From one tick to the next, which is also how tall each is to the pointer.
const TICK_PITCH: Pixels = px(8.);
/// Narrow enough to fit between the scrollbar and the rows, which are inset 28px from the pane's edge, so in a narrow
/// pane the ticks stay clear of a prompt at the right of the column.
const TICK_WIDTH: Pixels = px(10.);
const TICK_HEIGHT: Pixels = px(2.);
/// How faint a tick is with more past it.
const FADED: f32 = 0.35;
/// In from the pane's right edge, clear of the scrollbar's thumb, which reaches 12px in while it's dragged, and down
/// from the tabs.
const RIGHT: Pixels = px(14.);
const TOP: Pixels = px(20.);
/// How far above the ticks the list's edge is, so its first row starts level with them.
const LIST_ABOVE: Pixels = px(4.);
/// Over the rows and the composer, as a menu is.
const PRIORITY: usize = 1;

/// A prompt, and the row of the chat it's drawn in, which the log says, since the chat's rows go by the agent's own
/// events, or from a hub that can't read them apart, by every event.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Prompt {
    line: PromptLine,
    row: usize,
}

/// The prompts as the outline last read them, and the list's rows for them. A frame reads them once, so the ticks and
/// the list agree, and copies them only when the log's have changed.
#[derive(Default)]
pub(super) struct Prompts {
    changes: u64,
    lines: Rc<[Prompt]>,
    rows: list::Rows,
}

impl Prompts {
    fn read(&mut self, session: &OpenSession) -> Rc<[Prompt]> {
        let changed = session.read(|log| {
            let changed = log.prompt_lines_changed() != self.changes;
            if changed {
                let prompt = |line: &PromptLine| Prompt { row: log.prompt_row(line), line: line.clone() };
                self.lines = log.prompt_lines().map(prompt).collect();
                self.changes = log.prompt_lines_changed();
            }
            changed
        });
        if changed {
            self.rows.follow(&self.lines);
        }
        self.lines.clone()
    }
}

/// How a tick is drawn. The one being read is never at a faded end, since the ticks shown are around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tick {
    BeingRead,
    /// At an end with more past it.
    Faded,
    Plain,
}

/// What a click on a prompt in the list needs: the outline's state, to close the list, and the chat's list, to scroll,
/// and the transcript, to draw it there.
#[derive(Clone)]
struct Jumps {
    hover: Entity<Hover>,
    chat_list: TranscriptList,
    transcript: EntityId,
}

impl Jumps {
    /// Shows the prompt in `row` at the top of the chat, and closes the list, so it isn't over where it jumped to.
    fn to(&self, row: usize) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let Jumps { hover, chat_list, transcript } = self.clone();
        move |_, _, cx| {
            hover.update(cx, Hover::jumped);
            chat_list.go_to_row(row);
            cx.notify(transcript);
        }
    }
}

impl Transcript {
    /// The chat, with the ticks over it, and the list while it's open, if it has more than one prompt to move between.
    pub(super) fn with_outline(
        &mut self,
        chat: AnyElement,
        line_text: LineText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let around = div().relative().size_full();
        let lines = self.prompts.read(&self.session);
        if lines.len() < 2 {
            return around.child(chat);
        }
        let at = being_read(&lines, self.list.top().row, !self.list.is_scrolled_up());
        let hover = window.use_keyed_state("outline", cx, |_, _| Hover::default());
        let showing = hover.update(cx, |hover, _| hover.frame());
        if showing == Showing::Opened {
            self.prompts.rows.open_at(at);
        }
        let ticks = ticks(&lines, at, &hover, window, cx);
        let list = (showing != Showing::Closed).then(|| {
            let on_hover = window.listener_for(&hover, |h: &mut Hover, hovered: &bool, _, cx| {
                h.hovered(Part::List, *hovered, cx);
            });
            let jumps = Jumps { hover: hover.clone(), chat_list: self.list.clone(), transcript: cx.entity_id() };
            let list = list::draw(lines, at, &self.prompts.rows, jumps, on_hover, line_text, cx);
            // Over the ticks, which it hides while it's open, with its right edge on theirs.
            let over = div().absolute().top(-LIST_ABOVE).right_0().child(list);
            deferred(over).with_priority(PRIORITY)
        });
        around
            .child(tells_the_outline_of_the_wheel(&hover))
            .child(chat)
            .child(div().absolute().top(TOP).right(RIGHT).child(ticks).children(list))
    }
}

/// Tells the outline the wheel turned, so it knows when that was with the pointer on the ticks. The chat takes the
/// wheel before anything drawn over it hears it, and GPUI offers it first to what was drawn first, so this is drawn
/// before the chat, and draws nothing.
fn tells_the_outline_of_the_wheel(hover: &Entity<Hover>) -> AnyElement {
    let hover = hover.clone();
    canvas(
        |_, _, _| {},
        move |_, (), window, _| {
            window.on_mouse_event(move |_: &ScrollEvent, phase, _, cx| {
                if phase == DispatchPhase::Capture {
                    hover.update(cx, Hover::wheeled);
                }
            });
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

/// The ticks around the prompt being read. They take the pointer from what's under them, such as a tool call's line
/// reaching under them in a narrow pane, but let the wheel through to the chat. They're drawn while the list is open
/// too, under it, so they hear the pointer leave.
fn ticks(lines: &[Prompt], at: usize, hover: &Entity<Hover>, window: &mut Window, cx: &App) -> AnyElement {
    let shown = shown(lines.len(), at);
    let ticks: Vec<AnyElement> = shown
        .clone()
        .map(|ix| {
            let label = format!("Prompt {} of {}", ix + 1, lines.len());
            tick(lines[ix].row, label, look(ix, at, &shown, lines.len()), cx)
        })
        .collect();
    div()
        .region(Region::Outline)
        .role(Role::ListBox)
        .aria_label("Prompts")
        .w(TICK_WIDTH)
        .block_mouse_except_scroll()
        .on_hover(window.listener_for(hover, |h: &mut Hover, hovered: &bool, _, cx| {
            h.hovered(Part::Ticks, *hovered, cx);
        }))
        .on_mouse_move(window.listener_for(hover, |h: &mut Hover, _, _, cx| h.moved_over_ticks(cx)))
        .children(ticks)
        .into_any_element()
}

/// How the tick for prompt `ix` of `count` is drawn, with `at` being read and the ticks `shown`.
fn look(ix: usize, at: usize, shown: &Range<usize>, count: usize) -> Tick {
    if ix == at {
        Tick::BeingRead
    } else if (ix == shown.start && ix > 0) || (ix + 1 == shown.end && shown.end < count) {
        Tick::Faded
    } else {
        Tick::Plain
    }
}

fn tick(row: usize, label: String, look: Tick, cx: &App) -> AnyElement {
    let color = if look == Tick::BeingRead { cx.theme().foreground } else { look::faint() };
    div()
        .id(ElementId::Name(format!("outline-tick/{row}").into()))
        .role(Role::ListBoxOption)
        .aria_label(label)
        .aria_selected(look == Tick::BeingRead)
        .h(TICK_PITCH)
        .w_full()
        .flex()
        .items_center()
        .child(div().w_full().h(TICK_HEIGHT).rounded_full().bg(color).when(look == Tick::Faded, |d| d.opacity(FADED)))
        .test_support()
        .into_any_element()
}

/// Which prompt is being read, as an index into `lines`: the last one at or above the top row. At the end of the chat,
/// it's the last prompt, even if it starts lower down, since nothing after it can come to the top.
fn being_read(lines: &[Prompt], top: usize, at_end: bool) -> usize {
    if at_end {
        return lines.len().saturating_sub(1);
    }
    lines.partition_point(|prompt| prompt.row <= top).saturating_sub(1)
}

/// The ticks to show, as a range of `count` prompts: all of them if they fit, and otherwise as many as fit, around the
/// one being read.
fn shown(count: usize, at: usize) -> Range<usize> {
    if count <= MOST_TICKS {
        return 0..count;
    }
    let start = at.saturating_sub(MOST_TICKS / 2).min(count - MOST_TICKS);
    start..start + MOST_TICKS
}
