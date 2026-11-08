//! SKETCH, never merged: three choices of motif for Roundhouse's status mark, each shown live in a mock of the app
//! and on its own. Throwaway, for the design-system question.

use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, BorderStyle, Bounds, Context, Corners, Edges, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, PathBuilder, Pixels, Point, Render, Rgba, StatefulInteractiveElement, Styled, Window, canvas, div,
    point, px, quad, rgba, size, transparent_black,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Turntable,
    Stalls(Stall),
    Rails,
}

/// One way of drawing the fan of stalls: how many, and whether each is a solid segment or made of fine dots.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Stall {
    name: &'static str,
    bays: usize,
    /// Rows of dots out from the turntable. None means solid segments.
    rows: usize,
    layout: Layout,
    /// For segments, how thick, as a share of the mark's width.
    weight: f32,
    /// For segments, the gap between them, in radians.
    gap: f32,
}

/// How the dots sit.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Layout {
    /// One straight column per stall. The gaps widen toward the outside.
    Columns,
    /// Columns, with dots that grow toward the outside to fill the gaps.
    Grown,
    /// Spaced evenly along each row, so outer rows have more dots and the edge stays dense.
    Even,
    /// Even, with a thin empty spoke between stalls.
    EvenGapped,
}

const fn solid(name: &'static str, bays: usize, weight: f32, gap: f32) -> Stall {
    Stall { name, bays, rows: 0, layout: Layout::Columns, weight, gap }
}

const fn dots(name: &'static str, bays: usize, rows: usize, layout: Layout) -> Stall {
    Stall { name, bays, rows, layout, weight: 0.0, gap: 0.0 }
}

pub const STALLS: [Stall; 9] = [
    solid("5 bold", 5, 1.0 / 6.5, 0.16),
    solid("7", 7, 1.0 / 9.0, 0.11),
    solid("11 fine", 11, 1.0 / 14.0, 0.07),
    dots("13 × 4 columns", 13, 4, Layout::Columns),
    dots("13 × 4 growing", 13, 4, Layout::Grown),
    dots("Even, 3 rows", 7, 3, Layout::Even),
    dots("Even, 4 rows", 7, 4, Layout::Even),
    dots("Even, 5 rows", 7, 5, Layout::Even),
    dots("Even, 4 rows, 5 stalls", 5, 4, Layout::EvenGapped),
];

pub const KINDS: [Kind; 11] = [
    Kind::Stalls(STALLS[0]),
    Kind::Stalls(STALLS[1]),
    Kind::Stalls(STALLS[2]),
    Kind::Stalls(STALLS[3]),
    Kind::Stalls(STALLS[4]),
    Kind::Stalls(STALLS[5]),
    Kind::Stalls(STALLS[6]),
    Kind::Stalls(STALLS[7]),
    Kind::Stalls(STALLS[8]),
    Kind::Turntable,
    Kind::Rails,
];

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Turntable => "Turntable",
            Kind::Stalls(stall) => stall.name,
            Kind::Rails => "Rails",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Kind::Turntable => {
                "The turntable at the heart of a roundhouse, with tracks out to the stalls. While an agent works, the \
                 bridge turns from track to track."
            }
            Kind::Stalls(stall) if stall.rows > 0 => match stall.layout {
                Layout::Columns => {
                    "Stalls in fine dots, each stall a straight column out from the turntable. The gaps widen toward \
                     the outside."
                }
                Layout::Grown => "The same columns, with dots that grow toward the outside to fill the gaps.",
                Layout::Even => {
                    "Fine dots spaced evenly along each row, so outer rows have more of them and the edge stays as \
                     dense as the middle. While an agent works, a light sweeps the fan, with the outer rows trailing."
                }
                Layout::EvenGapped => {
                    "Evenly spaced dots, with a thin empty spoke between stalls, so the stalls can still be counted."
                }
            },
            Kind::Stalls(_) => {
                "A roundhouse from above: a fan of stalls around the turntable, one per job. While an agent works, a \
                 light moves from stall to stall."
            }
            Kind::Rails => "A length of track. While an agent works, it runs underneath, like riding a train.",
        }
    }

    /// Height over width.
    fn aspect(self) -> f32 {
        match self {
            Kind::Turntable => 1.0,
            Kind::Stalls(_) => 0.62,
            Kind::Rails => 0.5,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Signal {
    Idle,
    SettingUp,
    Working,
    NeedsYou,
    Done,
    Failed,
}

const SIGNALS: [Signal; 6] =
    [Signal::Idle, Signal::SettingUp, Signal::Working, Signal::NeedsYou, Signal::Done, Signal::Failed];

impl Signal {
    fn name(self) -> &'static str {
        match self {
            Signal::Idle => "Idle",
            Signal::SettingUp => "Setting up",
            Signal::Working => "Working",
            Signal::NeedsYou => "Needs you",
            Signal::Done => "Done, unread",
            Signal::Failed => "Stopped short",
        }
    }

    fn how(self, kind: Kind) -> &'static str {
        if let Kind::Stalls(stall) = kind
            && stall.rows > 0
            && self == Signal::NeedsYou
        {
            return "Amber, rippling outward";
        }
        match (kind, self) {
            (Kind::Turntable, Signal::Idle) => "Bridge at rest",
            (Kind::Turntable, Signal::SettingUp) => "Tracks laid one by one",
            (Kind::Turntable, Signal::Working) => "Bridge turns, track to track",
            (Kind::Turntable, Signal::NeedsYou) => "Amber, bridge toward you",
            (Kind::Turntable, Signal::Done) => "Copper, every track lit",
            (Kind::Turntable, Signal::Failed) => "Red, bridge broken",
            (Kind::Stalls(_), Signal::Idle) => "Stalls dark",
            (Kind::Stalls(_), Signal::SettingUp) => "Stalls built one by one",
            (Kind::Stalls(_), Signal::Working) => "A light, stall to stall",
            (Kind::Stalls(_), Signal::NeedsYou) => "Amber, breathing",
            (Kind::Stalls(_), Signal::Done) => "Copper, all stalls lit",
            (Kind::Stalls(_), Signal::Failed) => "Red, middle stall gone",
            (Kind::Rails, Signal::Idle) => "Track at rest",
            (Kind::Rails, Signal::SettingUp) => "Sleepers laid one by one",
            (Kind::Rails, Signal::Working) => "Track runs underneath",
            (Kind::Rails, Signal::NeedsYou) => "Amber, stopped, breathing",
            (Kind::Rails, Signal::Done) => "Copper, steady",
            (Kind::Rails, Signal::Failed) => "Red, rails broken",
        }
    }
}

fn hex(value: u32) -> Hsla {
    Hsla::from(Rgba::from(rgba(value)))
}

fn accent() -> Hsla {
    hex(0xd08a_5cff)
}

fn faint() -> Hsla {
    hex(0x6f69_64ff)
}

/// Which palette and which design of "stopped short" the whole app draws with.
static PALETTE: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

pub struct Palette {
    name: &'static str,
    about: &'static str,
    /// Idle, setting up, working, needs you, done, stopped short.
    colors: [u32; 6],
}

pub const PALETTES: [Palette; 5] = [
    Palette {
        name: "Copper (sketch 2)",
        about: "Everything active is copper, and amber and red sit close to it",
        colors: [0x6f6964, 0xd08a5c, 0xd08a5c, 0xe9b24c, 0xd08a5c, 0xee6f64],
    },
    Palette {
        name: "Signals",
        about: "Railway signals: green is clear, yellow is caution, red is stop. Copper stays for working",
        colors: [0x6f6964, 0x9aa7b8, 0xd08a5c, 0xf2c744, 0x5bc47e, 0xe5484d],
    },
    Palette {
        name: "Cool working",
        about: "Working is blue, so copper is only the brand. The widest spread",
        colors: [0x6f6964, 0x8b9cf6, 0x5aa9f5, 0xf0b440, 0x4cc38a, 0xf0545c],
    },
    Palette {
        name: "Warm, by brightness",
        about: "Stays warm. Done is near-white, waiting a clear yellow, failure a truer red",
        colors: [0x6f6964, 0x9c6b4c, 0xd08a5c, 0xf7cf4a, 0xf2ece6, 0xe5484d],
    },
    Palette {
        name: "Teal working",
        about: "Working is teal, against the warm theme, and copper means done",
        colors: [0x6f6964, 0x2f8f84, 0x3fc1b0, 0xf0b440, 0xd08a5c, 0xef5d68],
    },
];

pub const FAILED_STYLES: [(&str, &str); 5] = [
    ("Dark stall", "The middle stall goes dark"),
    ("Stopped partway", "Red up to where it stopped, the rest faint"),
    ("Stop lamp", "The fan dims, and the turntable lights up red"),
    ("Glitch", "A few dots knocked out of place"),
    ("Collapsed", "The inner rows stay, the outer rows fade away"),
];

fn palette_color(palette: usize, signal: Signal) -> Hsla {
    let i = match signal {
        Signal::Idle => 0,
        Signal::SettingUp => 1,
        Signal::Working => 2,
        Signal::NeedsYou => 3,
        Signal::Done => 4,
        Signal::Failed => 5,
    };
    hex((PALETTES[palette].colors[i] << 8) | 0xff)
}

fn signal_color(signal: Signal, _: &App) -> Hsla {
    palette_color(PALETTE.load(Ordering::Relaxed), signal)
}

fn hash(a: usize, b: usize) -> f32 {
    let mut x = (a as u32).wrapping_mul(73_856_093) ^ (b as u32).wrapping_mul(19_349_663);
    x ^= x >> 13;
    x = x.wrapping_mul(1_274_126_177);
    ((x >> 8) & 0xffff) as f32 / 65535.0
}

pub struct Motifs {
    pub kind: Kind,
    speed: f32,
    paused: bool,
    base_t: f32,
    base_at: Instant,
    /// A fixed time, for pictures. The clock doesn't run while it's set.
    pub fixed: Option<f32>,
    /// How wide the mark is at sidebar size.
    small: f32,
}

impl Motifs {
    pub fn new() -> Self {
        Self { kind: Kind::Stalls(STALLS[6]), speed: 1.0, paused: false, base_t: 0.0, base_at: Instant::now(), fixed: None, small: 18.0 }
    }

    fn now(&self) -> f32 {
        if let Some(t) = self.fixed {
            return t;
        }
        if self.paused { self.base_t } else { self.base_t + self.base_at.elapsed().as_secs_f32() * self.speed }
    }

    fn set_speed(&mut self, speed: f32) {
        self.base_t = self.now();
        self.base_at = Instant::now();
        self.speed = speed;
    }

    fn toggle_pause(&mut self) {
        self.base_t = self.now();
        self.base_at = Instant::now();
        self.paused = !self.paused;
    }
}

/// One workspace that goes through a whole job, so the changes between states can be seen.
fn scripted(t: f32) -> Signal {
    match t.rem_euclid(26.0) {
        x if x < 3.0 => Signal::SettingUp,
        x if x < 10.0 => Signal::Working,
        x if x < 15.0 => Signal::NeedsYou,
        x if x < 20.0 => Signal::Working,
        _ => Signal::Done,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Drawing the marks.

pub fn mark(kind: Kind, signal: Signal, width: f32, t: f32, cx: &App) -> impl IntoElement {
    mark_with(kind, signal, width, t, PALETTE.load(Ordering::Relaxed), FAILED.load(Ordering::Relaxed), cx)
}

fn mark_with(kind: Kind, signal: Signal, width: f32, t: f32, palette: usize, failed: usize, _: &App) -> impl IntoElement {
    let color = palette_color(palette, signal);
    let idle = palette_color(palette, Signal::Idle);
    canvas(|_, _, _| {}, move |bounds, _, window, _| paint(kind, signal, color, idle, failed, t, bounds, window))
        .flex_none()
        .w(px(width))
        .h(px(width * kind.aspect()))
}

/// The mark as the brand, in copper whatever the palette, for empty states and the app icon.
fn mark_brand(kind: Kind, width: f32, t: f32) -> impl IntoElement {
    let color = accent();
    canvas(|_, _, _| {}, move |bounds, _, window, _| paint(kind, Signal::Done, color, faint(), 0, t, bounds, window))
        .flex_none()
        .w(px(width))
        .h(px(width * kind.aspect()))
}

#[expect(clippy::too_many_arguments, reason = "a throwaway sketch")]
fn paint(
    kind: Kind,
    signal: Signal,
    color: Hsla,
    idle: Hsla,
    failed: usize,
    t: f32,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) {
    match kind {
        Kind::Turntable => turntable(signal, color, t, bounds, window),
        Kind::Stalls(stall) if stall.rows > 0 => dotted_stalls(stall, signal, color, idle, failed, t, bounds, window),
        Kind::Stalls(stall) => stalls(stall, signal, color, t, bounds, window),
        Kind::Rails => rails(signal, color, t, bounds, window),
    }
}

fn line(window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, width: Pixels, color: Hsla) {
    let mut path = PathBuilder::stroke(width);
    path.move_to(a);
    path.line_to(b);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn arc(window: &mut Window, c: Point<Pixels>, r: Pixels, a0: f32, a1: f32, width: Pixels, color: Hsla) {
    let at = |a: f32| point(c.x + r * a.cos(), c.y + r * a.sin());
    let mut path = PathBuilder::stroke(width);
    path.move_to(at(a0));
    path.arc_to(point(r, r), px(0.), (a1 - a0).abs() > PI, a1 > a0, at(a1));
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}

fn disc(window: &mut Window, c: Point<Pixels>, r: Pixels, color: Hsla) {
    let bounds = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
    window.paint_quad(quad(bounds, Corners::all(r), color, Edges::all(px(0.)), transparent_black(), BorderStyle::default()));
}

fn ring(window: &mut Window, c: Point<Pixels>, r: Pixels, width: Pixels, color: Hsla) {
    let bounds = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
    window.paint_quad(quad(bounds, Corners::all(r), transparent_black(), Edges::all(width), color, BorderStyle::default()));
}

fn breathe(t: f32) -> f32 {
    0.5 + 0.5 * (0.5 + 0.5 * (t * TAU / 1.8).cos())
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn stroke_for(width: Pixels) -> Pixels {
    (width / 12.0).clamp(px(1.25), px(3.0))
}

/// Tracks fan out over the top, as a roundhouse's stalls do, and the lead track comes in from below.
const TRACKS: [f32; 6] = [-0.9 * PI, -0.7 * PI, -0.5 * PI, -0.3 * PI, -0.1 * PI, 0.5 * PI];

fn turntable(signal: Signal, color: Hsla, t: f32, b: Bounds<Pixels>, window: &mut Window) {
    let s = b.size.width;
    let c = point(b.origin.x + s * 0.5, b.origin.y + s * 0.58);
    let w = stroke_for(s);
    let pit = s * 0.2;
    let (inner, outer) = (pit + w * 1.3, s * 0.5 - w * 0.4);
    let fade = if signal == Signal::NeedsYou { breathe(t) } else { 1.0 };
    let color = color.opacity(fade);
    // The bridge steps from one stall's track to the next.
    let step = 0.9;
    let working_angle = {
        let k = (t / step).floor();
        let f = (t / step).fract();
        -0.1 * PI + (k + smooth(f / 0.55)) * PI / 5.0
    };
    let bridge = match signal {
        Signal::Working => working_angle,
        Signal::NeedsYou => PI / 2.0,
        _ => -0.1 * PI,
    };
    let shown = ((t / 2.8).fract() * 7.0).floor() as usize;
    for (i, track) in TRACKS.iter().copied().enumerate() {
        let alpha = match signal {
            Signal::Working => {
                let d = (track - bridge).rem_euclid(PI);
                let d = d.min(PI - d);
                (1.0 - (d / (PI / 10.0)).min(1.0)).mul_add(0.7, 0.3)
            }
            Signal::NeedsYou if i == 5 => 1.0,
            Signal::NeedsYou => 0.5,
            Signal::SettingUp if i < shown => 0.9,
            Signal::SettingUp => 0.0,
            Signal::Failed => 0.45,
            Signal::Idle | Signal::Done => 1.0,
        };
        if alpha <= 0.0 {
            continue;
        }
        let (dx, dy) = (track.cos(), track.sin());
        let reach = if i == 5 { s * 0.42 - w * 0.4 } else { outer };
        line(window, point(c.x + inner * dx, c.y + inner * dy), point(c.x + reach * dx, c.y + reach * dy), w, color.opacity(alpha * fade));
    }
    let pit_alpha = if matches!(signal, Signal::SettingUp | Signal::Working) { 0.55 } else { 1.0 };
    ring(window, c, pit + w * 0.5, w * 0.8, color.opacity(pit_alpha * fade));
    let reach = pit + w * 0.9;
    if signal == Signal::Failed {
        // Two halves, knocked out of line.
        let tilt = 0.4_f32;
        line(window, point(c.x - reach, c.y + reach * tilt * 0.5), point(c.x - w * 0.7, c.y), w, color);
        line(window, point(c.x + w * 0.7, c.y), point(c.x + reach, c.y + reach * tilt * 0.5), w, color);
    } else {
        let (dx, dy) = (bridge.cos(), bridge.sin());
        let bridge_alpha = if signal == Signal::SettingUp { 0.5 } else { 1.0 };
        line(window, point(c.x - reach * dx, c.y - reach * dy), point(c.x + reach * dx, c.y + reach * dy), w * 1.2, color.opacity(bridge_alpha * fade));
    }
}

fn stalls(stall: Stall, signal: Signal, color: Hsla, t: f32, b: Bounds<Pixels>, window: &mut Window) {
    let wd = b.size.width;
    let h = b.size.height;
    let width = (wd * stall.weight).clamp(px(1.0), px(9.0));
    let c = point(b.origin.x + wd * 0.5, b.origin.y + h - width * 0.55);
    let r = (wd * 0.5 - width * 0.5).min(h - width * 1.1);
    let fade = if signal == Signal::NeedsYou { breathe(t) } else { 1.0 };
    let bays = stall.bays;
    let span = PI / bays as f32;
    let gap = stall.gap.min(span * 0.4);
    let pos = (t / 2.2).fract() * (bays as f32 + 1.0) - 0.5;
    let shown = ((t / 2.8).fract() * (bays as f32 + 1.0)).floor() as usize;
    let falloff = 3.75 / bays as f32;
    for k in 0..bays {
        let alpha = match signal {
            Signal::Working => (1.0 - (pos - k as f32).abs() * falloff).clamp(0.25, 1.0),
            Signal::SettingUp if k >= shown => 0.0,
            Signal::SettingUp => 0.6,
            Signal::Failed if k == bays / 2 => 0.0,
            _ => 1.0,
        };
        if alpha <= 0.0 {
            continue;
        }
        let a0 = -PI + k as f32 * span + gap * 0.5;
        arc(window, c, r, a0, a0 + span - gap, width, color.opacity(alpha * fade));
    }
    disc(window, c, (width * 0.55).max(px(1.2)), color.opacity(fade));
}

/// The fan as fine dots.
#[expect(clippy::too_many_arguments, reason = "a throwaway sketch")]
fn dotted_stalls(
    stall: Stall,
    signal: Signal,
    color: Hsla,
    idle: Hsla,
    failed: usize,
    t: f32,
    b: Bounds<Pixels>,
    window: &mut Window,
) {
    let wd = b.size.width;
    let h = b.size.height;
    let (bays, rows) = (stall.bays, stall.rows);
    let outer = (wd * 0.5).min(h * 0.94);
    let inner = outer * 0.4;
    let pitch = (outer - inner) / (rows as f32 - 0.4);
    let base = (pitch * 0.32).max(px(0.45));
    let c = point(b.origin.x + wd * 0.5, b.origin.y + h - base * 1.4);
    // Each dot: how far along the fan it is, from 0 to 1, its row, and its radius.
    let mut dots: Vec<(f32, usize, usize)> = Vec::new();
    match stall.layout {
        Layout::Columns | Layout::Grown => {
            for k in 0..bays {
                for i in 0..rows {
                    dots.push(((k as f32 + 0.5) / bays as f32, i, k));
                }
            }
        }
        Layout::Even | Layout::EvenGapped => {
            for i in 0..rows {
                let r = inner + pitch * i as f32;
                let n = ((PI * r) / pitch).round() as usize + 1;
                for j in 0..n {
                    let u = j as f32 / (n - 1) as f32;
                    if stall.layout == Layout::EvenGapped {
                        let sector = u * bays as f32;
                        let nearest = sector.round();
                        let spoke = (sector - nearest).abs() * PI * r / bays as f32;
                        if nearest > 0.0 && nearest < bays as f32 && spoke < pitch * 0.34 {
                            continue;
                        }
                    }
                    dots.push((u, i, j));
                }
            }
        }
    }
    let span = PI / bays as f32;
    let column_dot = (base).min(outer * span * 0.3);
    let middle = (bays / 2) as f32;
    let sweep = (t / 2.2).fract() * 1.5 - 0.25;
    for (u, i, j) in dots {
        let mut r = inner + pitch * i as f32;
        let size = match stall.layout {
            Layout::Columns => column_dot,
            Layout::Grown => column_dot * (r / inner).powf(0.5),
            Layout::Even | Layout::EvenGapped => base,
        };
        let sector = (u * bays as f32).floor().min(bays as f32 - 1.0);
        let alpha = match signal {
            Signal::Working => {
                let lag = sweep - i as f32 * 0.06;
                (1.0 - (u - lag).abs() * 3.5).clamp(0.18, 1.0)
            }
            Signal::SettingUp => {
                let built = (t / 3.0).fract() * 1.25;
                if u + i as f32 * 0.04 < built { 0.7 } else { 0.0 }
            }
            Signal::NeedsYou => 0.3 + 0.7 * (0.5 + 0.5 * (t * TAU / 1.6 - i as f32 * 1.1).cos()),
            _ => 1.0,
        };
        let mut dot_color = color;
        let alpha = if signal == Signal::Failed {
            match failed {
                0 if (sector - middle).abs() < 0.5 => 0.0,
                1 if u > 0.56 => {
                    dot_color = idle;
                    1.0
                }
                2 => 0.28,
                3 => {
                    let h = hash(i, j);
                    if h < 0.07 {
                        0.0
                    } else if h < 0.24 {
                        r += pitch * if h < 0.155 { 0.5 } else { -0.5 };
                        0.8
                    } else {
                        1.0
                    }
                }
                4 => match i {
                    0 | 1 => 1.0,
                    2 => 0.22,
                    _ => 0.07,
                },
                _ => 1.0,
            }
        } else {
            alpha
        };
        if alpha <= 0.0 {
            continue;
        }
        let angle = -PI + PI * u;
        disc(window, point(c.x + r * angle.cos(), c.y + r * angle.sin()), size, dot_color.opacity(alpha));
    }
    let hub_alpha = if signal == Signal::NeedsYou { breathe(t) } else { 1.0 };
    if signal == Signal::Failed && failed == 2 {
        disc(window, c, base * 3.0, color);
    } else {
        ring(window, c, base * 2.6, (base * 0.8).max(px(0.6)), color.opacity(hub_alpha));
        disc(window, c, base, color.opacity(hub_alpha));
    }
}

fn rails(signal: Signal, color: Hsla, t: f32, b: Bounds<Pixels>, window: &mut Window) {
    let wd = b.size.width;
    let h = b.size.height;
    let w = stroke_for(wd);
    let tie_w = (w * 0.7).max(px(1.0));
    let (x0, y0) = (b.origin.x, b.origin.y);
    let (top, bottom) = (y0 + h * 0.3, y0 + h * 0.7);
    let fade = if signal == Signal::NeedsYou { breathe(t) } else { 1.0 };
    let rail_alpha = if signal == Signal::SettingUp { 0.5 } else { 1.0 };
    let rail = color.opacity(rail_alpha * fade);
    if signal == Signal::Failed {
        let (l, r) = (x0 + wd * 0.42, x0 + wd * 0.58);
        let drop = h * 0.08;
        line(window, point(x0, top), point(l, top), w, rail);
        line(window, point(r, top + drop), point(x0 + wd, top + drop), w, rail);
        line(window, point(x0, bottom), point(l, bottom), w, rail);
        line(window, point(r, bottom + drop), point(x0 + wd, bottom + drop), w, rail);
    } else {
        line(window, point(x0, top), point(x0 + wd, top), w, rail);
        line(window, point(x0, bottom), point(x0 + wd, bottom), w, rail);
    }
    let spacing = wd / 3.0;
    let offset = if signal == Signal::Working { spacing * (t / 0.5).fract() } else { px(0.) };
    let shown = ((t / 2.8).fract() * 5.0).floor() as i32;
    let tie_alpha = 0.7;
    for k in 0..4 {
        let x = x0 + spacing * (k as f32 + 0.5) - offset;
        let edge = ((x - x0).min(x0 + wd - x) / (spacing * 0.5)).clamp(0.0, 1.0);
        let mut alpha = match signal {
            Signal::SettingUp if k >= shown => 0.0,
            Signal::Failed if k == 1 => 0.0,
            _ => tie_alpha,
        };
        if signal == Signal::Working {
            alpha *= edge;
        }
        if alpha <= 0.0 || x < x0 || x > x0 + wd {
            continue;
        }
        let drop = if signal == Signal::Failed && k > 1 { h * 0.08 } else { px(0.) };
        line(window, point(x, y0 + h * 0.12 + drop), point(x, y0 + h * 0.88 + drop), tie_w, color.opacity(alpha * fade));
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The mock app around them.

struct Row {
    name: &'static str,
    signal: Option<Signal>,
    selected: bool,
}

impl Motifs {
    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let kinds = KINDS.iter().enumerate().map(|(i, kind)| {
            let kind = *kind;
            Button::new(gpui_kit::ElementId::NamedInteger("kind".into(), i as u64))
                .small()
                .ghost()
                .label(kind.name())
                .selected(self.kind == kind)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.kind = kind;
                    cx.notify();
                }))
        });
        let sizes = [16.0_f32, 18.0, 20.0].map(|small| {
            Button::new(gpui_kit::ElementId::NamedInteger("size".into(), small as u64))
                .small()
                .ghost()
                .label(format!("{small:.0}px"))
                .selected((self.small - small).abs() < 0.01)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.small = small;
                    cx.notify();
                }))
        });
        let speeds = [(0.5, "½×"), (1.0, "1×"), (2.0, "2×")].map(|(speed, label)| {
            Button::new(gpui_kit::ElementId::Name(format!("speed-{label}").into()))
                .small()
                .ghost()
                .label(label)
                .selected((self.speed - speed).abs() < 0.01)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.set_speed(speed);
                    cx.notify();
                }))
        });
        h_flex()
            .flex_none()
            .w_full()
            .h(px(52.))
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(border)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Motif"))
            .child(h_flex().gap_1().children(kinds))
            .child(div().flex_1().min_w_0())
            .child(div().text_xs().text_color(muted).child("Size"))
            .child(h_flex().gap_1().children(sizes))
            .child(div().text_xs().text_color(muted).child("Speed"))
            .child(h_flex().gap_1().children(speeds))
            .child(
                Button::new("pause")
                    .small()
                    .label(if self.paused { "Play" } else { "Pause" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.toggle_pause();
                        cx.notify();
                    })),
            )
    }

    fn options_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let palette = PALETTE.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let palettes = PALETTES.iter().enumerate().map(|(i, p)| {
            Button::new(gpui_kit::ElementId::NamedInteger("palette".into(), i as u64))
                .small()
                .ghost()
                .label(p.name)
                .selected(palette == i)
                .on_click(cx.listener(move |_, _, _, cx| {
                    PALETTE.store(i, Ordering::Relaxed);
                    cx.notify();
                }))
        });
        let failures = FAILED_STYLES.iter().enumerate().map(|(i, (name, _))| {
            Button::new(gpui_kit::ElementId::NamedInteger("failed".into(), i as u64))
                .small()
                .ghost()
                .label(*name)
                .selected(failed == i)
                .on_click(cx.listener(move |_, _, _, cx| {
                    FAILED.store(i, Ordering::Relaxed);
                    cx.notify();
                }))
        });
        h_flex()
            .flex_none()
            .w_full()
            .h(px(44.))
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(border)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Colours"))
            .child(h_flex().gap_1().children(palettes))
            .child(div().w(px(12.)))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Stopped short"))
            .child(h_flex().gap_1().children(failures))
            .child(div().flex_1().min_w_0().text_xs().text_color(muted).overflow_hidden().whitespace_nowrap().text_ellipsis().child(
                format!("{}. {}.", PALETTES[palette].about, FAILED_STYLES[failed].1),
            ))
    }

    fn sidebar(&self, t: f32, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted, selected_bg) = (theme.sidebar, theme.sidebar_border, theme.muted_foreground, theme.sidebar_accent);
        let kind = self.kind;
        let small = self.small;
        let group = |name: &'static str, rows: Vec<Row>, offset: f32| {
            v_flex()
                .gap_px()
                .child(
                    h_flex()
                        .h(px(26.))
                        .pl_2()
                        .gap_1p5()
                        .text_xs()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(muted)
                        .child(Icon::new(IconName::Folder).xsmall().text_color(faint()))
                        .child(name),
                )
                .children(rows.into_iter().enumerate().map(move |(i, row)| {
                    let signal = row.signal.unwrap_or_else(|| scripted(t));
                    let local = t + offset + i as f32 * 0.37;
                    h_flex()
                        .h(px(30.))
                        .px_2()
                        .gap_2()
                        .rounded_md()
                        .text_sm()
                        .when(row.selected, |d| d.bg(selected_bg).font_weight(FontWeight::MEDIUM))
                        .child(div().w(px(small + 2.0)).flex().justify_center().child(mark(kind, signal, small, local, cx)))
                        .child(div().flex_1().child(row.name))
                        .when(row.signal.is_none(), |d| {
                            d.child(div().text_size(px(11.)).text_color(faint()).child(signal.name()))
                        })
                }))
        };
        v_flex()
            .flex_none()
            .w(px(260.))
            .h_full()
            .bg(bg)
            .border_r_1()
            .border_color(border)
            .px_2()
            .pt_4()
            .gap_4()
            .child(group(
                "roundhouse",
                vec![
                    Row { name: "alder", signal: Some(Signal::Idle), selected: false },
                    Row { name: "birch", signal: None, selected: false },
                    Row { name: "cedar", signal: Some(Signal::NeedsYou), selected: false },
                    Row { name: "aspen", signal: Some(Signal::Working), selected: true },
                ],
                0.0,
            ))
            .child(group(
                "signal-box",
                vec![
                    Row { name: "elm", signal: Some(Signal::Failed), selected: false },
                    Row { name: "fir", signal: Some(Signal::Working), selected: false },
                    Row { name: "hazel", signal: Some(Signal::SettingUp), selected: false },
                    Row { name: "juniper", signal: Some(Signal::Done), selected: false },
                ],
                1.3,
            ))
            .child(div().flex_1())
            .child(
                div()
                    .pb_3()
                    .px_2()
                    .text_xs()
                    .text_color(faint())
                    .child("birch goes through a whole job, about every 26 seconds."),
            )
    }

    fn pane(&self, t: f32, cx: &App) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted, raised, amber, surface, input, bg) =
            (theme.border, theme.muted_foreground, theme.secondary, theme.warning, theme.muted, theme.input, theme.background);
        let kind = self.kind;
        let small = self.small;
        let tab = |title: &'static str, signal: Signal, selected: bool, offset: f32| {
            h_flex()
                .h(px(28.))
                .pl_2()
                .pr_3()
                .gap_1p5()
                .rounded_md()
                .text_size(px(13.))
                .when(selected, |d| d.bg(raised))
                .when(!selected, |d| d.text_color(muted))
                .child(mark(kind, signal, small - 2.0, t + offset, cx))
                .child(title)
        };
        let option = |n: &'static str, label: &'static str, about: &'static str| {
            h_flex()
                .w_full()
                .px_2()
                .py_1p5()
                .gap_3()
                .items_start()
                .rounded_md()
                .border_1()
                .border_color(border)
                .bg(bg)
                .child(
                    div()
                        .flex_none()
                        .size(px(18.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .border_1()
                        .border_color(border)
                        .text_size(px(11.))
                        .text_color(muted)
                        .child(n),
                )
                .child(v_flex().gap_0p5().child(div().text_sm().child(label)).child(div().text_xs().text_color(muted).child(about)))
        };
        let column = v_flex()
            .w_full()
            .max_w(px(680.))
            .gap_4()
            .child(
                div()
                    .px_3p5()
                    .py_2p5()
                    .rounded_lg()
                    .border_1()
                    .border_color(border)
                    .bg(raised)
                    .text_sm()
                    .child("The sidebar should show each workspace's PR state. Where would that come from?"),
            )
            .child(div().text_sm().child("I'll start with how the sidebar gets its records."))
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .child(Icon::new(IconName::SquareTerminal).xsmall().text_color(faint()))
                    .child(div().font_weight(FontWeight::MEDIUM).child("Bash"))
                    .child(div().font_family("Menlo").text_color(muted).child("rg -n 'HubNotice::Workspace' crates apps")),
            )
            .child(
                v_flex()
                    .p_3()
                    .gap_3()
                    .rounded_lg()
                    .border_1()
                    .border_color(amber.opacity(0.45))
                    .bg(amber.opacity(0.05))
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(amber)
                            .child(mark(kind, Signal::NeedsYou, small - 2.0, t, cx))
                            .child("The agent is asking · PR state"),
                    )
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Where should the PR state be fetched?"))
                    .child(option("1", "In the hub", "Asked once, and every client gets it from the feed"))
                    .child(option("2", "In each client", "Simpler, but every window asks GitHub on its own")),
            )
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(muted)
                    .child(mark(kind, Signal::Working, small, t + 0.2, cx))
                    .child("Running cargo test -p desktop"),
            );
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .flex_none()
                    .h(px(48.))
                    .px_4()
                    .gap_2()
                    .border_b_1()
                    .border_color(border)
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("aspen"))
                    .child(div().text_color(faint()).child("/"))
                    .child(div().text_xs().font_family("Menlo").text_color(muted).child("sidebar-pr-state")),
            )
            .child(
                h_flex()
                    .flex_none()
                    .h(px(40.))
                    .px_3()
                    .gap_1()
                    .border_b_1()
                    .border_color(border)
                    .child(tab("Wire PR state into the feed", Signal::Working, true, 0.5))
                    .child(tab("Check the CLI fixture", Signal::NeedsYou, false, 0.0))
                    .child(tab("Chat 3", Signal::Idle, false, 0.0)),
            )
            .child(div().flex_1().min_h_0().px_6().pt_5().flex().justify_center().child(column))
            .child(
                div().flex_none().px_4().pb_4().child(
                    v_flex()
                        .rounded_lg()
                        .border_1()
                        .border_color(input)
                        .bg(surface)
                        .child(div().px_3().pt_2p5().h(px(52.)).text_sm().text_color(faint()).child("Ask the agent to do something"))
                        .child(
                            h_flex()
                                .px_2()
                                .pb_2()
                                .gap_1()
                                .text_xs()
                                .text_color(muted)
                                .child(div().px_2().child("Opus"))
                                .child(div().px_2().child("Accept edits"))
                                .child(div().flex_1())
                                .child(
                                    h_flex()
                                        .h(px(26.))
                                        .px_2()
                                        .gap_1p5()
                                        .rounded_md()
                                        .border_1()
                                        .border_color(border)
                                        .bg(raised)
                                        .text_color(theme.foreground)
                                        .child(mark(kind, Signal::Working, small - 2.0, t + 0.4, cx))
                                        .child("Stop"),
                                ),
                        ),
                ),
            )
    }

    fn sheet(&self, t: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted, fg, raised) =
            (theme.sidebar, theme.border, theme.muted_foreground, theme.foreground, theme.secondary);
        let (selected_bg, hover_bg) = (theme.sidebar_accent, theme.list_hover);
        let kind = self.kind;
        let small = self.small;
        let variants: Vec<AnyElement> = STALLS
            .iter()
            .enumerate()
            .map(|(i, stall)| {
                let this = Kind::Stalls(*stall);
                let selected = kind == this;
                v_flex()
                    .id(gpui_kit::ElementId::NamedInteger("variant".into(), i as u64))
                    .w(px(128.))
                    .items_center()
                    .gap_2()
                    .py_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(selected_bg))
                    .when(!selected, |d| d.hover(move |d| d.bg(hover_bg)))
                    .on_click(cx.listener(move |m, _, _, cx| {
                        m.kind = this;
                        cx.notify();
                    }))
                    .child(div().h(px(40.)).flex().items_center().child(mark(this, Signal::Working, 60.0, t + i as f32 * 0.21, cx)))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(mark(this, Signal::Working, small, t, cx))
                            .child(mark(this, Signal::NeedsYou, small, t, cx))
                            .child(mark(this, Signal::Failed, small, t, cx))
                            .child(mark(this, Signal::Idle, small, t, cx)),
                    )
                    .child(div().text_xs().text_color(if selected { fg } else { muted }).child(stall.name))
                    .into_any_element()
            })
            .collect();
        let states = SIGNALS.iter().map(|signal| {
            let signal = *signal;
            v_flex()
                .w(px(132.))
                .items_center()
                .gap_2()
                .py_3()
                .child(div().h(px(64.)).flex().items_center().child(mark(kind, signal, 60.0, t, cx)))
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(mark(kind, signal, small, t, cx))
                        .child(div().text_sm().text_color(fg).child(signal.name())),
                )
                .child(div().text_xs().text_center().text_color(muted).child(signal.how(kind)))
        });
        let palette_now = PALETTE.load(Ordering::Relaxed);
        let failed_now = FAILED.load(Ordering::Relaxed);
        let palette_rows: Vec<AnyElement> = PALETTES
            .iter()
            .enumerate()
            .map(|(p, palette)| {
                let selected = p == palette_now;
                h_flex()
                    .id(gpui_kit::ElementId::NamedInteger("palette-row".into(), p as u64))
                    .w_full()
                    .px_2()
                    .py_1p5()
                    .gap_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(selected_bg))
                    .when(!selected, |d| d.hover(move |d| d.bg(hover_bg)))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        PALETTE.store(p, Ordering::Relaxed);
                        cx.notify();
                    }))
                    .child(div().w(px(112.)).text_xs().text_color(if selected { fg } else { muted }).child(palette.name))
                    .children(SIGNALS.iter().map(|signal| {
                        div().w(px(45.)).flex().justify_center().child(mark_with(kind, *signal, 34.0, t, p, failed_now, cx))
                    }))
                    .into_any_element()
            })
            .collect();
        let failure_designs: Vec<AnyElement> = FAILED_STYLES
            .iter()
            .enumerate()
            .map(|(f, (name, how))| {
                let selected = f == failed_now;
                v_flex()
                    .id(gpui_kit::ElementId::NamedInteger("failed-design".into(), f as u64))
                    .w(px(128.))
                    .items_center()
                    .gap_2()
                    .py_3()
                    .rounded_lg()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(selected_bg))
                    .when(!selected, |d| d.hover(move |d| d.bg(hover_bg)))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        FAILED.store(f, Ordering::Relaxed);
                        cx.notify();
                    }))
                    .child(div().h(px(40.)).flex().items_center().child(mark_with(kind, Signal::Failed, 60.0, t, palette_now, f, cx)))
                    .child(mark_with(kind, Signal::Failed, small, t, palette_now, f, cx))
                    .child(div().text_xs().text_color(if selected { fg } else { muted }).child(*name))
                    .child(div().px_1().text_xs().text_center().text_color(faint()).child(*how))
                    .into_any_element()
            })
            .collect();
        let heading = |text: &'static str| div().text_xs().font_weight(FontWeight::MEDIUM).text_color(muted).child(text);
        let tile = |label: &'static str, body: AnyElement| {
            v_flex()
                .flex_1()
                .h(px(190.))
                .items_center()
                .justify_center()
                .gap_3()
                .rounded_lg()
                .border_1()
                .border_color(border)
                .bg(theme.background)
                .child(body)
                .child(div().text_xs().text_color(faint()).child(label))
        };
        v_flex()
            .id("sheet")
            .flex_none()
            .w(px(440.))
            .h_full()
            .overflow_y_scroll()
            .bg(bg)
            .border_l_1()
            .border_color(border)
            .p_4()
            .gap_4()
            .child(heading("Every palette, every state. Click one to use it everywhere"))
            .child(v_flex().gap_1().children(palette_rows))
            .child(heading("Five ways to show \"stopped short\". Click one to use it everywhere"))
            .child(h_flex().flex_wrap().justify_center().gap_1().children(failure_designs))
            .child(heading("Every state, large and at sidebar size"))
            .child(h_flex().flex_wrap().justify_center().children(states))
            .child(heading("Bigger moments"))
            .child(
                h_flex()
                    .gap_3()
                    .child(tile(
                        "Empty state",
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(mark_brand(kind, 44.0, t))
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Add a repository"))
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .rounded_md()
                                    .bg(theme.primary)
                                    .text_xs()
                                    .text_color(theme.primary_foreground)
                                    .child("Add repository"),
                            )
                            .into_any_element(),
                    ))
                    .child(tile(
                        "Starting up",
                        v_flex()
                            .items_center()
                            .gap_3()
                            .child(mark(kind, Signal::Working, 40.0, t, cx))
                            .child(div().text_xs().text_color(muted).child("Connecting to the hub…"))
                            .into_any_element(),
                    ))
                    .child(tile(
                        "App icon",
                        div()
                            .size(px(84.))
                            .rounded(px(19.))
                            .border_1()
                            .border_color(border)
                            .bg(raised)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(mark_brand(kind, 50.0, 0.0))
                            .into_any_element(),
                    )),
            )
            .child(heading("Every way of drawing the stalls, working"))
            .child(h_flex().flex_wrap().justify_center().gap_1().children(variants))
    }
}

impl Render for Motifs {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.now();
        let theme = cx.theme();
        let (bg, fg) = (theme.background, theme.foreground);
        let toolbar = self.toolbar(cx).into_any_element();
        let about = self.options_bar(cx).into_any_element();
        let body = h_flex()
            .flex_1()
            .min_h_0()
            .w_full()
            .child(self.sidebar(t, cx))
            .child(self.pane(t, cx))
            .child(self.sheet(t, cx));
        if self.fixed.is_none() && !self.paused {
            window.request_animation_frame();
        }
        v_flex().id("motifs").size_full().bg(bg).text_color(fg).child(toolbar).child(about).child(body)
    }
}
