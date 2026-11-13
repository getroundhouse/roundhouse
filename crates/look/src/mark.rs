//! The status mark: a half circle of fine dots around a turntable, like a roundhouse seen from above. It has three
//! layers, each about one thing. The fan's colour is the work, where the workspace's pull request stands, and nothing
//! else changes it. A chat's fan is grey, since the pull request is the workspace's. A white light over it is the
//! agent: sweeping the fan while it works, and kept to the inner row once its turn has ended with something still
//! running. And the turntable is about you: red when the agent stopped short. A chat waiting on you, or with a reply
//! you haven't read, isn't on the mark, since the name beside it says to look.

use gpui_kit::{App, Bounds, Hsla, IntoElement, Pixels, Styled, Window, canvas, px};

/// The two sizes a mark comes in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    /// In the sidebar.
    Row,
    /// Beside text: chat tabs, Home's rows and the working line.
    Inline,
}

impl Size {
    /// How wide a mark of this size is drawn, for lining things up with it.
    pub fn width(self) -> Pixels {
        match self {
            Size::Row => px(26.),
            Size::Inline => px(16.),
        }
    }
}

use crate::clock;
use crate::colors::colors;
use crate::fan::{self, ASPECT, Dot, Fan, as_f32};
use crate::work::Work;

/// What the agent is doing, and whether it needs you.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Signal {
    /// Not running. Nothing moves.
    Idle,
    /// Being set up. The fan fills in, in white.
    SettingUp,
    /// Working. A white light sweeps the fan, the outer rows trailing.
    Working,
    /// Its turn has ended with something still running that isn't the agent's own work, such as a command or a
    /// monitor. The light keeps to the inner row, beside the turntable, which is lit, and goes slower: something is
    /// alive there, and it isn't the agent.
    Background,
    /// Stopped short. The turntable turns red.
    Failed,
}

/// The moment, in seconds, a mark that isn't moving is drawn at. It catches a working mark with its light on the fan.
pub(crate) const STILL_S: f64 = 0.9;
const SWEEP_S: f64 = 2.4;
/// How long the light of something in the background takes to cross the inner row, or to go round a task's ring.
/// Slower than a working agent's sweep, so the two aren't taken for each other.
pub(crate) const BACKGROUND_S: f64 = 3.6;
const BUILD_S: f64 = 4.8;
/// How far along the fan each row of a working mark trails the one inside it.
const TRAIL: f32 = 0.06;
/// How strongly the agent's light shows over the fan's colour, where it's brightest.
pub(crate) const LIGHT: f32 = 0.92;

/// A workspace's mark, whose fan is in its pull request's colour.
pub fn mark(signal: Signal, work: Work, size: Size, cx: &App) -> impl IntoElement {
    let colors = colors(cx);
    // Only the pull request colours the fan, so its colour means one thing. A reply nobody has read, or a question
    // waiting on an answer, isn't on the mark at all: the name beside it says to look.
    let color = work.fan(colors);
    let hub = match signal {
        Signal::Failed => colors.stopped,
        Signal::Background => colors.text.opacity(LIGHT),
        Signal::Idle | Signal::SettingUp | Signal::Working => unlit(color),
    };
    let moves = matches!(signal, Signal::Working | Signal::Background | Signal::SettingUp) && clock::moving(cx);
    let t = if moves { clock::elapsed() } else { STILL_S };
    drawn(signal, color, hub, size.width(), t, moves, cx)
}

/// A chat's mark, which says only what its agent is doing. A pull request is the workspace's, not one chat's, so the
/// fan is grey, and the workspace's mark shows the pull request.
pub fn chat_mark(signal: Signal, size: Size, cx: &App) -> impl IntoElement {
    mark(signal, Work::NoPr, size, cx)
}

/// The mark in copper with nothing on it: the brand, for empty states. Copper is the theme's focus ring, because
/// focus is the one other place the brand colour shows. The app icon is the same fan, drawn for each of its sizes.
pub fn brand(width: Pixels, cx: &App) -> impl IntoElement {
    let copper = colors(cx).brand;
    drawn(Signal::Idle, copper, unlit(copper), width, STILL_S, false, cx)
}

/// A turntable, or a ring's hub, with nothing to say: in the dots' colour, and quieter than them.
pub(crate) fn unlit(fan: Hsla) -> Hsla {
    fan.opacity(0.6)
}

fn drawn(signal: Signal, color: Hsla, hub: Hsla, width: Pixels, t: f64, moves: bool, cx: &App) -> impl IntoElement {
    let light = colors(cx).text;
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            paint(bounds, signal, color, hub, light, t, window);
            // Drawn again on the display's next frame, for as long as it's on screen and moving.
            if moves {
                window.request_animation_frame();
            }
        },
    )
    .flex_none()
    .w(width)
    .h(width * ASPECT)
}

fn paint(bounds: Bounds<Pixels>, signal: Signal, color: Hsla, hub: Hsla, light: Hsla, t: f64, window: &mut Window) {
    let fan = Fan::within(bounds);
    for dot in fan.dots() {
        let (base, glow) = brightness(signal, &dot, t);
        fan::disc(window, dot.at, fan.dot, color.opacity(base));
        if glow > 0.01 {
            fan::disc(window, dot.at, fan.dot, light.opacity(glow * LIGHT));
        }
    }
    let turntable = fan.turntable();
    fan::ring(window, fan.center, turntable.radius, turntable.line, hub);
    fan::disc(window, fan.center, turntable.hub, hub);
}

/// How strongly a dot shows its colour, and how much of the agent's white light is on it, at `t` seconds.
fn brightness(signal: Signal, dot: &Dot, t: f64) -> (f32, f32) {
    let row = as_f32(dot.row);
    match signal {
        Signal::Working => (1.0, light_on(dot.along, dot.row, phase(t, SWEEP_S) * 1.5 - 0.25, TRAIL)),
        Signal::Background if dot.row == 0 => (1.0, light_on(dot.along, 0, phase(t, BACKGROUND_S) * 1.5 - 0.25, 0.0)),
        Signal::SettingUp if dot.along + row * 0.04 < phase(t, BUILD_S) * 1.25 => (1.0, 0.4),
        Signal::SettingUp => (0.2, 0.0),
        Signal::Idle | Signal::Background | Signal::Failed => (1.0, 0.0),
    }
}

/// How much of the agent's light is on a dot `along` the fan in `row`, from 0 to 1, when the light is at `sweep` along
/// the inner row: full on the dot under it and fading out to either side, with each row out `trail` further behind.
pub(crate) fn light_on(along: f32, row: usize, sweep: f32, trail: f32) -> f32 {
    (1.0 - (along - (sweep - as_f32(row) * trail)).abs() * 3.5).clamp(0.0, 1.0)
}

/// How far through a motion that repeats every `period_s` seconds it is, from 0 to 1, at `t` seconds.
#[expect(clippy::cast_possible_truncation, reason = "a fraction from 0 to 1")]
pub(crate) fn phase(t: f64, period_s: f64) -> f32 {
    (t / period_s).fract() as f32
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{Fan, Signal, brightness};

    #[test]
    fn only_an_agent_with_something_running_moves_the_mark() {
        let fan = Fan::within(Bounds::new(point(px(0.), px(0.)), size(px(60.), px(37.2))));
        let at = |signal, t| fan.dots().map(|dot| brightness(signal, &dot, t)).collect::<Vec<_>>();

        for still in [Signal::Idle, Signal::Failed] {
            assert_eq!(at(still, 0.3), at(still, 1.7), "{still:?} moved");
        }
        for moving in [Signal::Working, Signal::Background, Signal::SettingUp] {
            assert_ne!(at(moving, 0.3), at(moving, 1.7), "{moving:?} held still");
        }
    }

    #[test]
    fn work_in_the_background_lights_only_the_inner_row() {
        let fan = Fan::within(Bounds::new(point(px(0.), px(0.)), size(px(60.), px(37.2))));
        let lit = |signal, t| fan.dots().filter(|dot| brightness(signal, dot, t).1 > 0.0).map(|dot| dot.row).max();

        for t in [0.3, 0.9, 1.7, 2.6] {
            assert_eq!(lit(Signal::Background, t), Some(0), "at {t}");
        }
        assert!(lit(Signal::Working, 0.9) > Some(0), "a working agent's reaches the rows outside it");
    }
}
