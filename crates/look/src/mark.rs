//! The status mark: a half circle of fine dots around a turntable, like a roundhouse seen from above. It has three
//! layers, each about one thing. The fan's colour is the work, where its pull request stands. A white light over
//! it is the agent, while it's doing something. And when you're needed, or the agent stopped short, the whole fan
//! turns amber or red.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{App, Bounds, Hsla, IntoElement, Pixels, Styled, Window, canvas, px};

/// The two sizes a mark comes in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    /// In the sidebar.
    Row,
    /// Beside text: chat tabs, cards and the working line.
    Inline,
}

impl Size {
    fn width(self) -> Pixels {
        match self {
            Size::Row => px(18.),
            Size::Inline => px(16.),
        }
    }
}

use crate::clock;
use crate::fan::{self, ASPECT, Dot, Fan, as_f32};
use crate::palette::{faint, hex};

/// The work: where its pull request stands, in GitHub's colours. The fan's colour while nobody needs you.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Work {
    NoPr,
    Draft,
    Open,
    Merged,
    Closed,
}

impl Work {
    /// From GitHub's own words for a pull request's state. Anything else is taken as no pull request.
    pub fn from_github(state: &str, is_draft: bool) -> Self {
        match state {
            "OPEN" if is_draft => Work::Draft,
            "OPEN" => Work::Open,
            "MERGED" => Work::Merged,
            "CLOSED" => Work::Closed,
            _ => Work::NoPr,
        }
    }

    /// What to call it beside a pull request's number.
    pub fn name(self) -> &'static str {
        match self {
            Work::NoPr => "No pull request",
            Work::Draft => "Draft",
            Work::Open => "Open",
            Work::Merged => "Merged",
            Work::Closed => "Closed",
        }
    }

    fn color(self) -> Hsla {
        match self {
            Work::NoPr => faint(),
            Work::Draft => hex(0x008b_949e),
            Work::Open => hex(0x003f_b950),
            Work::Merged => hex(0x00a3_71f7),
            Work::Closed => hex(0x00b9_605a),
        }
    }
}

/// What the agent is doing, and whether it needs you.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Signal {
    /// Not running. Nothing moves.
    Idle,
    /// Being set up. The fan fills in, in white.
    SettingUp,
    /// Working. A white light sweeps the fan, the outer rows trailing.
    Working,
    /// Waiting on you, for an answer or an approval. The whole fan turns amber and holds still, because motion
    /// means the agent is running.
    NeedsYou,
    /// Stopped short. The whole fan turns red.
    Failed,
}

/// The moment, in seconds, a mark that isn't moving is drawn at. It catches a working mark with its light on the fan.
const STILL_S: f64 = 0.9;
const SWEEP_S: f64 = 2.4;
const BUILD_S: f64 = 4.8;

/// The mark for one workspace or chat.
pub fn mark(signal: Signal, work: Work, size: Size, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let color = match signal {
        Signal::NeedsYou => theme.warning,
        Signal::Failed => theme.danger,
        Signal::Idle | Signal::SettingUp | Signal::Working => work.color(),
    };
    let moves = matches!(signal, Signal::Working | Signal::SettingUp) && clock::moving(cx);
    let t = if moves { clock::elapsed() } else { STILL_S };
    drawn(signal, color, size.width(), t, moves, cx)
}

/// The mark in copper with nothing on it: the brand, for empty states. Copper is the theme's focus ring, because
/// focus is the one other place the brand colour shows. The app icon is the same fan, drawn for each of its sizes.
pub fn brand(width: Pixels, cx: &App) -> impl IntoElement {
    drawn(Signal::Idle, cx.theme().ring, width, STILL_S, false, cx)
}

fn drawn(signal: Signal, color: Hsla, width: Pixels, t: f64, moves: bool, cx: &App) -> impl IntoElement {
    let light = cx.theme().foreground;
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            paint(bounds, signal, color, light, t, window);
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

fn paint(bounds: Bounds<Pixels>, signal: Signal, color: Hsla, light: Hsla, t: f64, window: &mut Window) {
    let fan = Fan::within(bounds);
    for dot in fan.dots() {
        let (base, glow) = brightness(signal, &dot, t);
        fan::disc(window, dot.at, fan.dot, color.opacity(base));
        if glow > 0.01 {
            fan::disc(window, dot.at, fan.dot, light.opacity(glow * 0.92));
        }
    }
    let hub = color.opacity(0.6);
    let turntable = fan.turntable();
    fan::ring(window, fan.center, turntable.radius, turntable.line, hub);
    fan::disc(window, fan.center, turntable.hub, hub);
}

/// How strongly a dot shows its colour, and how much of the agent's white light is on it, at `t` seconds.
fn brightness(signal: Signal, dot: &Dot, t: f64) -> (f32, f32) {
    let row = as_f32(dot.row);
    match signal {
        Signal::Working => {
            let sweep = phase(t, SWEEP_S) * 1.5 - 0.25 - row * 0.06;
            (1.0, (1.0 - (dot.along - sweep).abs() * 3.5).clamp(0.0, 1.0))
        }
        Signal::SettingUp if dot.along + row * 0.04 < phase(t, BUILD_S) * 1.25 => (1.0, 0.4),
        Signal::SettingUp => (0.2, 0.0),
        Signal::Idle | Signal::NeedsYou | Signal::Failed => (1.0, 0.0),
    }
}

/// How far through a motion that repeats every `period_s` seconds it is, from 0 to 1, at `t` seconds.
#[expect(clippy::cast_possible_truncation, reason = "a fraction from 0 to 1")]
fn phase(t: f64, period_s: f64) -> f32 {
    (t / period_s).fract() as f32
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{Fan, Signal, Work, brightness};

    #[test]
    fn a_pull_request_is_read_from_githubs_words() {
        assert_eq!(Work::from_github("OPEN", false), Work::Open);
        assert_eq!(Work::from_github("OPEN", true), Work::Draft);
        assert_eq!(Work::from_github("MERGED", false), Work::Merged);
        assert_eq!(Work::from_github("CLOSED", false), Work::Closed);
        assert_eq!(Work::from_github("SOMETHING_NEW", false), Work::NoPr);
    }

    #[test]
    fn only_a_running_agent_moves_the_mark() {
        let fan = Fan::within(Bounds::new(point(px(0.), px(0.)), size(px(60.), px(37.2))));
        let at = |signal, t| fan.dots().map(|dot| brightness(signal, &dot, t)).collect::<Vec<_>>();

        for still in [Signal::Idle, Signal::NeedsYou, Signal::Failed] {
            assert_eq!(at(still, 0.3), at(still, 1.7), "{still:?} moved");
        }
        for moving in [Signal::Working, Signal::SettingUp] {
            assert_ne!(at(moving, 0.3), at(moving, 1.7), "{moving:?} held still");
        }
    }
}
