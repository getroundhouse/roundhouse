//! Every state the status mark can show, in one picture: each thing its agent can be doing, against each colour a
//! pull request gives its fan, at the sidebar's size and at Home's. A tab's mark is the `None` one at Home's size,
//! since a chat's mark doesn't show the pull request. Under them, the ring a background task has, and the robot a
//! subagent at work in the background has, each drawn by the app's own code, held still as the screens hold them.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    Context, Div, IntoElement, ParentElement as _, PlatformTextSystem, Render, Styled as _, Window, div, px, size,
};
use look::{Signal, Size, TaskRing, Work};

use super::capture::Picture;
use super::headless;

/// Its image's name, in `docs/spec/screens/`.
pub(crate) const NAME: &str = "mark-states";
/// The spec's file that shows it.
pub(crate) const SPEC: &str = "look.md";
const LABEL: f32 = 250.;
const CELL: f32 = 112.;
const ROW: f32 = 40.;
const MARGIN: f32 = 16.;

const SIGNALS: [Signal; 5] = [Signal::Idle, Signal::Working, Signal::SettingUp, Signal::Background, Signal::Failed];
const WORKS: [Work; 7] = [Work::NoPr, Work::Draft, Work::Open, Work::Ready, Work::Queued, Work::Merged, Work::Closed];
const RINGS: [TaskRing; 2] = [TaskRing::Running, TaskRing::Watching];

/// Matched in full, so a new signal doesn't compile until it has a name here, beside the list that draws it.
fn signal_name(signal: Signal) -> &'static str {
    match signal {
        Signal::Idle => "Nothing going on",
        Signal::Working => "The agent working",
        Signal::SettingUp => "Being set up",
        Signal::Background => "Something still running",
        Signal::Failed => "The agent stopped short",
    }
}

fn work_name(work: Work) -> &'static str {
    match work {
        Work::NoPr => "None",
        Work::Draft => "Draft",
        Work::Open => "Open",
        Work::Ready => "Ready",
        Work::Queued => "In the queue",
        Work::Merged => "Merged",
        Work::Closed => "Closed",
    }
}

fn ring_name(ring: TaskRing) -> &'static str {
    match ring {
        TaskRing::Running => "A background task running",
        TaskRing::Watching => "A monitor watching",
    }
}

struct Sheet;

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let label = |text: &'static str| div().w(px(LABEL)).text_sm().text_color(muted).child(text);
        let cell = || h_flex().w(px(CELL)).h(px(ROW)).gap_3();
        let heads: Vec<Div> =
            WORKS.iter().map(|work| cell().text_sm().text_color(muted).child(work_name(*work))).collect();
        let marks = SIGNALS.iter().map(|signal| {
            let across = WORKS.iter().map(|work| {
                cell().child(look::mark(*signal, *work, Size::Row, cx)).child(look::mark(
                    *signal,
                    *work,
                    Size::Inline,
                    cx,
                ))
            });
            h_flex().child(label(signal_name(*signal))).children(across.collect::<Vec<_>>())
        });
        let rings = RINGS
            .iter()
            .map(|ring| h_flex().child(label(ring_name(*ring))).child(cell().child(look::task_ring(*ring, cx))));
        let robot = h_flex().child(label("A subagent at work")).child(cell().child(look::robot_at_work(cx)));
        v_flex()
            .size_full()
            .bg(theme.sidebar)
            .p(px(MARGIN))
            .child(h_flex().child(label("Its pull request")).children(heads))
            .children(marks.collect::<Vec<_>>())
            .children(rings.collect::<Vec<_>>())
            .child(robot)
    }
}

/// At the app's scale, as the screens are.
pub(crate) fn draw(text_system: &Arc<dyn PlatformTextSystem>) -> Picture {
    let mut cx = headless::app(text_system);
    cx.update(|cx| look::hold_still(0, cx));
    #[expect(clippy::cast_precision_loss, reason = "a dozen rows and columns")]
    let (rows, columns) = ((1 + SIGNALS.len() + RINGS.len() + 1) as f32, WORKS.len() as f32);
    let width = px(MARGIN * 2. + LABEL + columns * CELL);
    let height = px(MARGIN * 2. + rows * ROW);
    headless::draw(&mut cx, size(width, height), |_| Sheet)
}
