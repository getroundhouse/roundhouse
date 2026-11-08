//! Roundhouse's own marks: the colours the kit's theme has no name for, and the fan of stalls, the app's motif.
//!
//! The mark has three layers, each about one thing. The fan's colour is the work: where its pull request stands.
//! A white light over it is the agent, while it's doing something. And when you're needed, or the agent stopped
//! short, the whole fan turns amber or red.
//!
//! SKETCH for the design-system question. In a real build this would be its own crate, since the desktop app and
//! the transcript both draw it.

use std::f32::consts::PI;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, BorderStyle, Bounds, Corners, Edges, Hsla, IntoElement, Pixels, Point, Rgba, Styled, Window, canvas, point,
    px, quad, rgba, size, transparent_black,
};

fn hex(value: u32) -> Hsla {
    Hsla::from(Rgba::from(rgba((value << 8) | 0xff)))
}

/// The brand colour: empty states, the app icon and focus. Never a status. Copper.
pub fn accent() -> Hsla {
    hex(0xd08a5c)
}

/// Quieter than muted: line numbers, idle marks, meta.
pub fn faint() -> Hsla {
    hex(0x6f6964)
}

/// A surface one step up from the pane it's on: messages, cards, the composer.
pub fn raised(cx: &App) -> Hsla {
    cx.theme().secondary
}

/// The work: where its pull request stands, in GitHub's colours. The fan's colour at rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Work {
    NoPr,
    Draft,
    Open,
    Merged,
    Closed,
}

impl Work {
    /// From GitHub's own words for it.
    pub fn from_github(state: &str, is_draft: bool) -> Self {
        match state {
            "OPEN" if is_draft => Work::Draft,
            "OPEN" => Work::Open,
            "MERGED" => Work::Merged,
            "CLOSED" => Work::Closed,
            _ => Work::NoPr,
        }
    }

    fn color(self) -> Hsla {
        match self {
            Work::NoPr => faint(),
            Work::Draft => hex(0x8b949e),
            Work::Open => hex(0x3fb950),
            Work::Merged => hex(0xa371f7),
            Work::Closed => hex(0xb9605a),
        }
    }
}

/// What the agent is doing, and whether it needs you.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Signal {
    /// Not running. Nothing on the fan.
    Idle,
    /// Being set up. The fan fills in, in white.
    SettingUp,
    /// Working. A white light sweeps the fan, the outer rows trailing.
    Working,
    /// Waiting on you: a question or an approval. The whole fan turns amber, and holds still, since motion means
    /// the agent is running.
    NeedsYou,
    /// Stopped short. The whole fan turns red.
    Failed,
}

/// The moment pictures are taken at, in seconds, so they come out the same every time.
pub const STILL: f32 = 0.9;

/// Every animation repeats within this, so a recording of it loops cleanly.
pub const CYCLE: f32 = 4.8;
const SWEEP: f32 = 2.4;
const BUILD: f32 = 4.8;

const ROWS: usize = 4;

/// The fan of stalls, as it looks in a picture. `width` is its width, and it's about 0.62 as tall.
pub fn fan(signal: Signal, work: Work, width: Pixels, cx: &App) -> impl IntoElement {
    fan_at(signal, work, width, STILL, cx)
}

/// The fan of stalls at `t` seconds into its animation.
pub fn fan_at(signal: Signal, work: Work, width: Pixels, t: f32, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let color = match signal {
        Signal::NeedsYou => theme.warning,
        Signal::Failed => theme.danger,
        Signal::Idle | Signal::SettingUp | Signal::Working => work.color(),
    };
    let light = theme.foreground;
    canvas(|_, _, _| {}, move |bounds, _, window, _| paint_fan(bounds, signal, color, light, t, window))
        .flex_none()
        .w(width)
        .h(width * 0.62)
}

/// The fan in copper, with nothing on it: the brand, for empty states and the app icon.
pub fn brand(width: Pixels, cx: &App) -> impl IntoElement {
    let light = cx.theme().foreground;
    canvas(|_, _, _| {}, move |bounds, _, window, _| paint_fan(bounds, Signal::Idle, accent(), light, STILL, window))
        .flex_none()
        .w(width)
        .h(width * 0.62)
}

fn disc(window: &mut Window, c: Point<Pixels>, r: Pixels, color: Hsla) {
    let bounds = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
    window.paint_quad(quad(bounds, Corners::all(r), color, Edges::all(px(0.)), transparent_black(), BorderStyle::default()));
}

fn ring(window: &mut Window, c: Point<Pixels>, r: Pixels, width: Pixels, color: Hsla) {
    let bounds = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
    window.paint_quad(quad(bounds, Corners::all(r), transparent_black(), Edges::all(width), color, BorderStyle::default()));
}

/// A half circle of fine dots around a turntable, in four rows. The dots are spaced evenly along each row, so outer
/// rows have more of them and the edge stays as dense as the middle.
fn paint_fan(b: Bounds<Pixels>, signal: Signal, color: Hsla, light: Hsla, t: f32, window: &mut Window) {
    let (wd, h) = (b.size.width, b.size.height);
    let outer = (wd * 0.5).min(h * 0.94);
    let inner = outer * 0.4;
    let pitch = (outer - inner) / (ROWS as f32 - 0.4);
    let dot = (pitch * 0.32).max(px(0.45));
    let c = point(b.origin.x + wd * 0.5, b.origin.y + h - dot * 1.4);
    let sweep = (t / SWEEP).fract() * 1.5 - 0.25;
    let built = (t / BUILD).fract() * 1.25;
    for i in 0..ROWS {
        let r = inner + pitch * i as f32;
        let n = ((PI * r) / pitch).round() as usize + 1;
        for j in 0..n {
            let u = j as f32 / (n - 1) as f32;
            let (base, glow) = match signal {
                Signal::Working => (1.0, (1.0 - (u - (sweep - i as f32 * 0.06)).abs() * 3.5).clamp(0.0, 1.0)),
                Signal::SettingUp if u + i as f32 * 0.04 < built => (1.0, 0.4),
                Signal::SettingUp => (0.2, 0.0),
                Signal::Idle | Signal::NeedsYou | Signal::Failed => (1.0, 0.0),
            };
            let angle = -PI + PI * u;
            let at = point(c.x + r * angle.cos(), c.y + r * angle.sin());
            disc(window, at, dot, color.opacity(base));
            if glow > 0.01 {
                disc(window, at, dot, light.opacity(glow * 0.92));
            }
        }
    }
    ring(window, c, dot * 2.6, (dot * 0.8).max(px(0.6)), color.opacity(0.6));
    disc(window, c, dot, color.opacity(0.6));
}
