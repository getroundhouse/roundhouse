//! The outline: a short tick at the right of the chat for each prompt, with the one being read in white, so a long chat
//! shows how far it goes and where you are in it, and a dot for each compaction, where the agent swapped what it held
//! for a summary. Resting the pointer on them lists the prompts and compactions over them, and clicking one jumps to
//! it. The ticks themselves aren't clicked, so they can stay small.

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
use protocol::model::{CompactionLine, PromptLine};

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
const TICK_PITCH: Pixels = px(12.);
/// The ticks are at least this many ticks tall to the pointer, so one or two are as easy to rest on as three. The
/// room past the last tick is empty.
const FEWEST_TICKS_TALL: f32 = 3.;
/// Narrow enough to fit between the scrollbar and the rows, which are inset 28px from the pane's edge, so in a narrow
/// pane, as at the size the app opens at, a prompt at the right of the column has a gap beside its ticks, rather than
/// seeming to touch them.
const TICK_WIDTH: Pixels = px(12.);
const TICK_HEIGHT: Pixels = px(3.);
/// A compaction's dot, a different shape from a tick's, so it reads as neither a prompt nor a gap.
const DOT: Pixels = px(4.);
/// How faint a tick is with more past it.
const FADED: f32 = 0.35;
/// In from the pane's right edge, clear by a pixel of the scrollbar's thumb, which reaches 12px in while it's dragged,
/// and down from the tabs.
const RIGHT: Pixels = px(13.);
const TOP: Pixels = px(20.);
/// How far above the ticks the list's edge is, so its first row starts level with them.
const LIST_ABOVE: Pixels = px(4.);
/// Over the rows and the composer, as a menu is.
const PRIORITY: usize = 1;

/// A prompt or a compaction, and the row of the chat it's drawn in, which the log says, since the chat's rows go by the
/// agent's own events, or from a hub that can't read them apart, by every event.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Line {
    kind: Kind,
    row: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Kind {
    /// The prompt `number` places from the first, counting from 1. Compactions aren't counted.
    Prompt {
        line: PromptLine,
        number: usize,
    },
    Compaction(CompactionLine),
}

impl Line {
    fn is_prompt(&self) -> bool {
        matches!(self.kind, Kind::Prompt { .. })
    }
}

/// The prompts and compactions as the outline last read them, in the order they're drawn in the chat, and the list's
/// rows for them. A frame reads them once, so the ticks and the list agree, and copies them only when the log's have
/// changed.
#[derive(Default)]
pub(super) struct Outline {
    changes: u64,
    lines: Rc<[Line]>,
    rows: list::Rows,
}

impl Outline {
    fn read(&mut self, session: &OpenSession) -> Rc<[Line]> {
        let changed = session.read(|log| {
            let changed = log.outline_changes() != self.changes;
            if changed {
                let prompts = log.prompt_lines().zip(1..).map(|(line, number)| Line {
                    row: log.prompt_row(line),
                    kind: Kind::Prompt { line: line.clone(), number },
                });
                let compactions = log
                    .compaction_lines()
                    .map(|line| Line { row: log.compaction_row(line), kind: Kind::Compaction(line.clone()) });
                let mut lines: Vec<Line> = prompts.chain(compactions).collect();
                // Stable, so a compaction a hub without places puts in a prompt's row stays after it.
                lines.sort_by_key(|line| line.row);
                self.lines = lines.into();
                self.changes = log.outline_changes();
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
    /// Shows the prompt or compaction in `row` at the bottom of the chat, with what came before it above it, and
    /// closes the list, so it isn't over where it jumped to.
    fn to(&self, row: usize) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let Jumps { hover, chat_list, transcript } = self.clone();
        move |_, _, cx| {
            hover.update(cx, Hover::jumped);
            chat_list.go_to_row_at_bottom(row);
            cx.notify(transcript);
        }
    }
}

impl Transcript {
    /// The chat, with the ticks over it, and the list while it's open, once it has a prompt. One prompt is enough,
    /// since a long turn takes it far above the screen, and the list is the way back up to it. Compactions alone
    /// aren't, since one is never the prompt being read.
    pub(super) fn with_outline(
        &mut self,
        chat: AnyElement,
        line_text: LineText,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let around = div().relative().size_full();
        let lines = self.outline.read(&self.session);
        if !lines.iter().any(Line::is_prompt) {
            return around.child(chat);
        }
        let at = being_read(&lines, self.list.bottom_row(), !self.list.is_scrolled_up());
        let hover = window.use_keyed_state("outline", cx, |_, _| Hover::default());
        let showing = hover.update(cx, |hover, _| hover.frame());
        if showing == Showing::Opened {
            self.outline.rows.open_at(at);
        }
        let ticks = ticks(&lines, at, &hover, window, cx);
        let list = (showing != Showing::Closed).then(|| {
            let on_hover = window.listener_for(&hover, |h: &mut Hover, hovered: &bool, _, cx| {
                h.hovered(Part::List, *hovered, cx);
            });
            let jumps = Jumps { hover: hover.clone(), chat_list: self.list.clone(), transcript: cx.entity_id() };
            let list = list::draw(lines, at, &self.outline.rows, jumps, on_hover, line_text, cx);
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

/// The ticks and dots around the prompt being read. They take the pointer from what's under them, such as a tool
/// call's line reaching under them in a narrow pane, but let the wheel through to the chat. They're drawn while the
/// list is open too, under it, so they hear the pointer leave.
fn ticks(lines: &[Line], at: usize, hover: &Entity<Hover>, window: &mut Window, cx: &App) -> AnyElement {
    let shown = shown(lines.len(), at);
    let prompts = lines.iter().filter(|line| line.is_prompt()).count();
    let ticks: Vec<AnyElement> =
        shown.clone().map(|ix| tick(&lines[ix], prompts, look(ix, at, &shown, lines.len()), cx)).collect();
    div()
        .region(Region::Outline)
        .role(Role::ListBox)
        .aria_label("Prompts")
        .w(TICK_WIDTH)
        .min_h(TICK_PITCH * FEWEST_TICKS_TALL)
        .block_mouse_except_scroll()
        .on_hover(window.listener_for(hover, |h: &mut Hover, hovered: &bool, _, cx| {
            h.hovered(Part::Ticks, *hovered, cx);
        }))
        .on_mouse_move(window.listener_for(hover, |h: &mut Hover, _, _, cx| h.moved_over_ticks(cx)))
        .children(ticks)
        .into_any_element()
}

/// How the tick or dot for line `ix` of `count` is drawn, with `at` being read and the ticks `shown`.
fn look(ix: usize, at: usize, shown: &Range<usize>, count: usize) -> Tick {
    if ix == at {
        Tick::BeingRead
    } else if (ix == shown.start && ix > 0) || (ix + 1 == shown.end && shown.end < count) {
        Tick::Faded
    } else {
        Tick::Plain
    }
}

/// A prompt's tick, or a compaction's dot, of the chat's `prompts`.
fn tick(line: &Line, prompts: usize, look: Tick, cx: &App) -> AnyElement {
    let color = if look == Tick::BeingRead { cx.theme().foreground } else { look::faint() };
    let (id, label, mark) = match &line.kind {
        Kind::Prompt { number, .. } => {
            let tick = div().w_full().h(TICK_HEIGHT);
            ("outline-tick", format!("Prompt {number} of {prompts}"), tick)
        }
        Kind::Compaction(compaction) => {
            ("outline-dot", crate::rows::compaction::who(compaction.by).to_string(), div().size(DOT))
        }
    };
    div()
        .id(ElementId::Name(format!("{id}/{}", line.row).into()))
        .role(Role::ListBoxOption)
        .aria_label(label)
        .aria_selected(look == Tick::BeingRead)
        .h(TICK_PITCH)
        .w_full()
        .flex()
        .items_center()
        .justify_center()
        .child(mark.rounded_full().bg(color).when(look == Tick::Faded, |d| d.opacity(FADED)))
        .test_support()
        .into_any_element()
}

/// Which prompt is being read, as an index into `lines`: the last one at or above the bottom row, whose part of the
/// chat is at the bottom, or before any has reached it, the first. At the end of the chat, it's the last prompt, as its
/// row makes it once the end is laid out, and also before then, when the list is first drawn. A compaction is never
/// the one being read, so the white tick is always a prompt's.
fn being_read(lines: &[Line], bottom: usize, at_end: bool) -> usize {
    let reached = if at_end { lines.len() } else { lines.partition_point(|line| line.row <= bottom) };
    lines[..reached].iter().rposition(Line::is_prompt).or_else(|| lines.iter().position(Line::is_prompt)).unwrap_or(0)
}

/// The ticks and dots to show, as a range of `count` lines: all of them if they fit, and otherwise as many as fit,
/// around the prompt being read.
fn shown(count: usize, at: usize) -> Range<usize> {
    if count <= MOST_TICKS {
        return 0..count;
    }
    let start = at.saturating_sub(MOST_TICKS / 2).min(count - MOST_TICKS);
    start..start + MOST_TICKS
}
