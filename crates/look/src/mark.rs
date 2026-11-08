//! The status mark: a half circle of fine dots around a turntable, like a roundhouse seen from above. It has three
//! layers, each about one thing. The fan's colour is the work, where its pull request stands. A white light over
//! it is the agent, while it's doing something. And when you're needed, or the agent stopped short, the whole fan
//! turns amber or red.

use std::f32::consts::PI;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, IntoElement, Pixels, Point, Styled, Window, canvas, point, px,
    quad, size, transparent_black,
};

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

/// The moment in the animation every mark is drawn at, in seconds. *Not built:* the marks don't move yet.
/// The moment, in seconds, a mark that isn't moving is drawn at. It catches a working mark with its light on the fan.
const STILL_S: f64 = 0.9;
const SWEEP_S: f64 = 2.4;
const BUILD_S: f64 = 4.8;
const ROWS: usize = 4;
/// How tall the mark is, for its width: a half circle, with room under it for the turntable.
const ASPECT: f32 = 0.62;
/// The inner row's radius, as a share of the outer row's. Inside it is the turntable.
const INNER: f32 = 0.4;
/// A dot's radius, as a share of the space between rows.
const DOT: f32 = 0.32;
/// The turntable's ring, in dot radii.
const TURNTABLE: f32 = 2.6;

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

/// The mark in copper with nothing on it: the brand, for empty states and the app icon. Copper is the theme's
/// focus ring, because focus is the one other place the brand colour shows.
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

/// One dot: where it is, how far along the fan from left to right, from 0 to 1, and which row, from the inside.
struct Dot {
    at: Point<Pixels>,
    along: f32,
    row: usize,
}

/// Where everything sits inside the mark's bounds.
struct Fan {
    center: Point<Pixels>,
    inner: Pixels,
    pitch: Pixels,
    dot: Pixels,
}

impl Fan {
    fn within(bounds: Bounds<Pixels>) -> Self {
        let outer = bounds.size.width * 0.5;
        let inner = outer * INNER;
        let pitch = (outer - inner) / (as_f32(ROWS) - 0.4);
        let dot = (pitch * DOT).max(px(0.45));
        let center = point(bounds.origin.x + bounds.size.width * 0.5, bounds.origin.y + bounds.size.height - dot * 1.4);
        Fan { center, inner, pitch, dot }
    }

    /// Spaced evenly along each row, so outer rows have more dots and the edge stays as dense as the middle.
    fn dots(&self) -> impl Iterator<Item = Dot> + '_ {
        (0..ROWS).flat_map(move |row| {
            let r = self.inner + self.pitch * as_f32(row);
            let count = dots_in((r * PI) / self.pitch) + 1;
            (0..count).map(move |j| {
                let along = as_f32(j) / as_f32(count - 1);
                let angle = PI * (along - 1.0);
                Dot { at: point(self.center.x + r * angle.cos(), self.center.y + r * angle.sin()), along, row }
            })
        })
    }
}

fn paint(bounds: Bounds<Pixels>, signal: Signal, color: Hsla, light: Hsla, t: f64, window: &mut Window) {
    let fan = Fan::within(bounds);
    for dot in fan.dots() {
        let (base, glow) = brightness(signal, &dot, t);
        disc(window, dot.at, fan.dot, color.opacity(base));
        if glow > 0.01 {
            disc(window, dot.at, fan.dot, light.opacity(glow * 0.92));
        }
    }
    let hub = color.opacity(0.6);
    let bounds = square(fan.center, fan.dot * TURNTABLE);
    window.paint_quad(quad(
        bounds,
        Corners::all(fan.dot * TURNTABLE),
        transparent_black(),
        Edges::all((fan.dot * 0.8).max(px(0.6))),
        hub,
        BorderStyle::default(),
    ));
    disc(window, fan.center, fan.dot, hub);
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

fn square(center: Point<Pixels>, radius: Pixels) -> Bounds<Pixels> {
    Bounds::new(point(center.x - radius, center.y - radius), size(radius * 2.0, radius * 2.0))
}

fn disc(window: &mut Window, center: Point<Pixels>, radius: Pixels, color: Hsla) {
    window.paint_quad(quad(
        square(center, radius),
        Corners::all(radius),
        color,
        Edges::all(px(0.)),
        transparent_black(),
        BorderStyle::default(),
    ));
}

/// How far through a motion that repeats every `period_s` seconds it is, from 0 to 1, at `t` seconds.
#[expect(clippy::cast_possible_truncation, reason = "a fraction from 0 to 1")]
fn phase(t: f64, period_s: f64) -> f32 {
    (t / period_s).fract() as f32
}

#[expect(clippy::cast_precision_loss, reason = "a count of rows or dots, a few dozen at most")]
fn as_f32(n: usize) -> f32 {
    n as f32
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "how many dots fit along a row: positive, and a few dozen at most"
)]
fn dots_in(length_in_pitches: f32) -> usize {
    length_in_pitches.round().max(1.0) as usize
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{Fan, ROWS, Signal, Work, brightness};

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

    #[test]
    fn outer_rows_have_more_dots_and_every_row_reaches_both_ends() {
        let fan = Fan::within(Bounds::new(point(px(0.), px(0.)), size(px(60.), px(37.2))));
        let dots: Vec<_> = fan.dots().collect();
        let per_row: Vec<usize> = (0..ROWS).map(|row| dots.iter().filter(|d| d.row == row).count()).collect();

        assert!(per_row.windows(2).all(|pair| pair[0] < pair[1]), "{per_row:?}");
        for row in 0..ROWS {
            let along: Vec<f32> = dots.iter().filter(|d| d.row == row).map(|d| d.along).collect();
            assert_eq!((along.first().copied(), along.last().copied()), (Some(0.0), Some(1.0)));
        }
    }
}
