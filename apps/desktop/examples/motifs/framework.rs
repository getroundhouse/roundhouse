//! SKETCH, never merged: the status mark as three layers, each about one thing. The fan's colour is the work, where
//! its PR stands. A white light over it is the agent, while it's doing something. The turntable in the middle is
//! you, lit when you're needed.

use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// Whether a closed PR's fan is GitHub's red, or the grey of no PR.
static CLOSED_RED: AtomicBool = AtomicBool::new(true);
/// Whether a draft PR's fan has a grey of its own, or the grey of no PR.
static DRAFT_OWN: AtomicBool = AtomicBool::new(true);

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, BorderStyle, Bounds, Context, Corners, Edges, ElementId, FontWeight, Hsla, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, Render, Rgba, StatefulInteractiveElement, Styled, Window, canvas, div,
    point, px, quad, rgba, size, transparent_black,
};

fn hex(value: u32) -> Hsla {
    Hsla::from(Rgba::from(rgba((value << 8) | 0xff)))
}

fn faint() -> Hsla {
    hex(0x6f6964)
}

fn copper() -> Hsla {
    hex(0xd08a5c)
}

fn white() -> Hsla {
    hex(0xf6f1ec)
}

fn amber() -> Hsla {
    hex(0xf2b84b)
}

fn red() -> Hsla {
    hex(0xf0524f)
}

// ---------------------------------------------------------------------------------------------------------------
// The three layers.

/// The work: where its pull request stands. The fan's colour, at rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pr {
    None,
    Draft,
    Open,
    Merged,
    Closed,
}

const PRS: [Pr; 5] = [Pr::None, Pr::Draft, Pr::Open, Pr::Merged, Pr::Closed];

impl Pr {
    fn name(self) -> &'static str {
        match self {
            Pr::None => "No PR",
            Pr::Draft => "Draft",
            Pr::Open => "Open",
            Pr::Merged => "Merged",
            Pr::Closed => "Closed",
        }
    }

    fn color(self) -> Hsla {
        match self {
            Pr::None => faint(),
            Pr::Draft if DRAFT_OWN.load(Ordering::Relaxed) => hex(0x8b949e),
            Pr::Draft => faint(),
            Pr::Open => hex(0x3fb950),
            Pr::Merged => hex(0xa371f7),
            Pr::Closed if CLOSED_RED.load(Ordering::Relaxed) => hex(0xb9605a),
            Pr::Closed => faint(),
        }
    }
}

/// The agent: what it's doing. A white light over the fan.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Agent {
    Idle,
    SettingUp,
    Working,
}

const AGENTS: [Agent; 3] = [Agent::Idle, Agent::SettingUp, Agent::Working];

impl Agent {
    fn name(self) -> &'static str {
        match self {
            Agent::Idle => "Not running",
            Agent::SettingUp => "Setting up",
            Agent::Working => "Working",
        }
    }
}

/// You: whether you're needed. The turntable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum You {
    Nothing,
    NeedsYou,
    Failed,
}

const YOUS: [You; 3] = [You::Nothing, You::NeedsYou, You::Failed];

impl You {
    fn name(self) -> &'static str {
        match self {
            You::Nothing => "Nothing",
            You::NeedsYou => "Your turn",
            You::Failed => "Stopped short",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct State {
    pub pr: Pr,
    pub agent: Agent,
    pub you: You,
}

const fn state(pr: Pr, agent: Agent, you: You) -> State {
    State { pr, agent, you }
}

/// How the turntable gets your eye when it's your turn.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Attention {
    Still,
    PulseOnce,
    Breathing,
}

const ATTENTIONS: [(Attention, &str); 3] =
    [(Attention::Still, "Still"), (Attention::PulseOnce, "One pulse when it starts"), (Attention::Breathing, "Breathing")];

/// Where the colour goes when it's your turn, or the agent stopped short. All of them stay in dots.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lamp {
    /// A small arc of dots around the centre.
    DottedTurntable,
    /// The turntable's thin outline.
    Ring,
    /// The row nearest the centre.
    InnerRow,
    /// The outermost row, the biggest arc.
    OuterRim,
    /// Every dot, over the PR colour.
    WholeFan,
}

const LAMPS: [(Lamp, &str); 5] = [
    (Lamp::DottedTurntable, "Dotted turntable"),
    (Lamp::Ring, "Ring"),
    (Lamp::InnerRow, "Inner row"),
    (Lamp::OuterRim, "Outer rim"),
    (Lamp::WholeFan, "Whole fan"),
];

/// When and how a mark is drawn.
#[derive(Clone, Copy)]
struct Show {
    t: f32,
    /// Seconds since it became your turn.
    since: f32,
    attention: Attention,
    lamp: Lamp,
}

// ---------------------------------------------------------------------------------------------------------------
// Drawing.

fn mark(state: State, width: f32, show: Show) -> impl IntoElement {
    canvas(|_, _, _| {}, move |bounds, _, window, _| paint(bounds, state, show, window))
        .flex_none()
        .w(px(width))
        .h(px(width * 0.62))
}

/// The brand: the fan in copper, with nothing on it.
fn brand(width: f32) -> impl IntoElement {
    canvas(|_, _, _| {}, move |bounds, _, window, _| paint_fan(bounds, copper(), 1.0, None, 0.0, window))
        .flex_none()
        .w(px(width))
        .h(px(width * 0.62))
}

fn disc(window: &mut Window, c: Point<Pixels>, r: Pixels, color: Hsla) {
    let bounds = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
    window.paint_quad(quad(bounds, Corners::all(r), color, Edges::all(px(0.)), transparent_black(), BorderStyle::default()));
}

fn ring(window: &mut Window, c: Point<Pixels>, r: Pixels, width: Pixels, color: Hsla) {
    let bounds = Bounds::new(point(c.x - r, c.y - r), size(r * 2.0, r * 2.0));
    window.paint_quad(quad(bounds, Corners::all(r), transparent_black(), Edges::all(width), color, BorderStyle::default()));
}

struct Geometry {
    c: Point<Pixels>,
    inner: Pixels,
    pitch: Pixels,
    dot: Pixels,
}

const ROWS: usize = 4;

fn geometry(b: Bounds<Pixels>) -> Geometry {
    let (wd, h) = (b.size.width, b.size.height);
    let outer = (wd * 0.5).min(h * 0.94);
    let inner = outer * 0.4;
    let pitch = (outer - inner) / (ROWS as f32 - 0.4);
    let dot = (pitch * 0.32).max(px(0.45));
    Geometry { c: point(b.origin.x + wd * 0.5, b.origin.y + h - dot * 1.4), inner, pitch, dot }
}

/// The fan in `color`, with the agent's light over it when there is one.
fn paint_fan(b: Bounds<Pixels>, color: Hsla, alpha: f32, agent: Option<Agent>, t: f32, window: &mut Window) {
    let g = geometry(b);
    let sweep = (t / 2.4).fract() * 1.5 - 0.25;
    let built = (t / 4.8).fract() * 1.25;
    for i in 0..ROWS {
        let r = g.inner + g.pitch * i as f32;
        let n = ((PI * r) / g.pitch).round() as usize + 1;
        for j in 0..n {
            let u = j as f32 / (n - 1) as f32;
            let (base, light) = match agent {
                Some(Agent::Working) => (alpha, (1.0 - (u - (sweep - i as f32 * 0.06)).abs() * 3.5).clamp(0.0, 1.0)),
                Some(Agent::SettingUp) if u + i as f32 * 0.04 < built => (alpha, 0.4),
                Some(Agent::SettingUp) => (alpha * 0.2, 0.0),
                Some(Agent::Idle) | None => (alpha, 0.0),
            };
            let angle = -PI + PI * u;
            let at = point(g.c.x + r * angle.cos(), g.c.y + r * angle.sin());
            disc(window, at, g.dot, color.opacity(base));
            if light > 0.01 {
                disc(window, at, g.dot, white().opacity(light * 0.92));
            }
        }
    }
}

fn paint(b: Bounds<Pixels>, state: State, show: Show, window: &mut Window) {
    let g = geometry(b);
    let base = state.pr.color();
    let alpha = if state.pr == Pr::None { 1.0 } else { 0.9 };
    let lamp = match state.you {
        You::Nothing => None,
        You::NeedsYou => Some(amber()),
        You::Failed => Some(red()),
    };
    let waiting = state.you == You::NeedsYou;
    let strength = if waiting && show.attention == Attention::Breathing {
        0.45 + 0.55 * (0.5 + 0.5 * (show.t * TAU / 1.8).cos())
    } else {
        1.0
    };
    // One ripple outward through the rows, when it first becomes your turn.
    let ripple = (waiting && show.attention == Attention::PulseOnce && show.since < 1.4)
        .then(|| show.since / 1.4 * (ROWS as f32 + 2.0) - 1.0);
    let lit_row = |i: usize| match show.lamp {
        Lamp::InnerRow => i == 0,
        Lamp::OuterRim => i == ROWS - 1,
        Lamp::WholeFan => true,
        Lamp::DottedTurntable | Lamp::Ring => false,
    };
    let sweep = (show.t / 2.4).fract() * 1.5 - 0.25;
    let built = (show.t / 4.8).fract() * 1.25;
    for i in 0..ROWS {
        let r = g.inner + g.pitch * i as f32;
        let n = ((PI * r) / g.pitch).round() as usize + 1;
        for j in 0..n {
            let u = j as f32 / (n - 1) as f32;
            let (fan, light) = match state.agent {
                Agent::Working => (alpha, (1.0 - (u - (sweep - i as f32 * 0.06)).abs() * 3.5).clamp(0.0, 1.0)),
                Agent::SettingUp if u + i as f32 * 0.04 < built => (alpha, 0.4),
                Agent::SettingUp => (alpha * 0.2, 0.0),
                Agent::Idle => (alpha, 0.0),
            };
            let angle = -PI + PI * u;
            let at = point(g.c.x + r * angle.cos(), g.c.y + r * angle.sin());
            let filling = show.lamp == Lamp::WholeFan;
            let reached = !filling || ripple.is_none_or(|front| front >= i as f32 - 0.2);
            match lamp {
                Some(color) if lit_row(i) && reached => disc(window, at, g.dot, color.opacity(strength)),
                _ => disc(window, at, g.dot, base.opacity(fan)),
            }
            if let (Some(front), Some(color)) = (ripple, lamp) {
                let wave = (1.0 - (front - i as f32).abs()).clamp(0.0, 1.0);
                if wave > 0.01 {
                    let edge = if filling { white() } else { color };
                    disc(window, at, g.dot, edge.opacity(wave * if filling { 0.75 } else { 0.9 }));
                }
            }
            if light > 0.01 {
                disc(window, at, g.dot, white().opacity(light * 0.92));
            }
        }
    }
    // The turntable.
    let hub = match (show.lamp, lamp) {
        (Lamp::DottedTurntable | Lamp::Ring, Some(color)) => color.opacity(strength),
        _ => base.opacity(0.55),
    };
    match show.lamp {
        Lamp::DottedTurntable => {
            let r = (g.inner - g.pitch * 0.85).max(g.dot * 2.2);
            let n = (((PI * r) / (g.pitch * 0.8)).round() as usize).max(3) + 1;
            for j in 0..n {
                let angle = -PI + PI * j as f32 / (n - 1) as f32;
                disc(window, point(g.c.x + r * angle.cos(), g.c.y + r * angle.sin()), g.dot, hub);
            }
            disc(window, g.c, g.dot, hub);
        }
        Lamp::Ring if lamp.is_some() => {
            ring(window, g.c, g.dot * 2.8, (g.dot * 1.15).max(px(0.9)), hub);
            disc(window, g.c, g.dot, hub);
        }
        _ => {
            ring(window, g.c, g.dot * 2.6, (g.dot * 0.8).max(px(0.6)), hub);
            disc(window, g.c, g.dot, hub);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The app.

/// One workspace that goes through a whole job, so the changes can be seen. Returns its state, and when it became
/// your turn.
fn journey(t: f32) -> (State, f32, &'static str) {
    let x = t.rem_euclid(30.0);
    let start = t - x;
    match x {
        x if x < 3.0 => (state(Pr::None, Agent::SettingUp, You::Nothing), 0.0, "Setting up"),
        x if x < 10.0 => (state(Pr::None, Agent::Working, You::Nothing), 0.0, "Working"),
        x if x < 15.0 => (state(Pr::None, Agent::Idle, You::NeedsYou), start + 10.0, "Asks you something"),
        x if x < 19.0 => (state(Pr::None, Agent::Working, You::Nothing), 0.0, "Working"),
        x if x < 22.0 => (state(Pr::Open, Agent::Working, You::Nothing), 0.0, "Opened a PR, still working"),
        x if x < 26.0 => (state(Pr::Open, Agent::Idle, You::Nothing), 0.0, "Done, PR open"),
        _ => (state(Pr::Merged, Agent::Idle, You::Nothing), 0.0, "Merged"),
    }
}

struct Row {
    name: &'static str,
    /// None is the workspace that goes through a whole job.
    state: Option<State>,
    selected: bool,
}

pub struct Framework {
    pub fixed: Option<f32>,
    start: Instant,
    paused_at: Option<f32>,
    small: f32,
    attention: Attention,
    lamp: Lamp,
    /// When "your turn" last began for the marks that are always waiting, so the pulse can be seen again.
    asked_at: f32,
    picked: State,
}

impl Framework {
    pub fn new() -> Self {
        Self {
            fixed: None,
            start: Instant::now(),
            paused_at: None,
            small: 18.0,
            attention: Attention::Still,
            lamp: Lamp::WholeFan,
            asked_at: 0.0,
            picked: state(Pr::Merged, Agent::Working, You::Nothing),
        }
    }

    fn now(&self) -> f32 {
        self.fixed.or(self.paused_at).unwrap_or_else(|| self.start.elapsed().as_secs_f32())
    }

    fn show(&self, t: f32) -> Show {
        Show { t, since: t - self.asked_at, attention: self.attention, lamp: self.lamp }
    }

    fn toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let attentions = ATTENTIONS.map(|(attention, label)| {
            Button::new(ElementId::Name(format!("attention-{label}").into()))
                .small()
                .ghost()
                .label(label)
                .selected(self.attention == attention)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.attention = attention;
                    this.asked_at = this.now();
                    cx.notify();
                }))
        });
        let sizes = [16.0_f32, 18.0, 20.0].map(|small| {
            Button::new(ElementId::NamedInteger("size".into(), small as u64))
                .small()
                .ghost()
                .label(format!("{small:.0}px"))
                .selected((self.small - small).abs() < 0.01)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.small = small;
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
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Status marks"))
            .child(div().text_xs().text_color(muted).child("Fan colour: the work's PR · White light: the agent · Whole fan amber or red: you"))
            .child(div().flex_1())
            .child(div().text_xs().text_color(muted).child("Your turn"))
            .child(h_flex().gap_1().children(attentions))
            .child(
                Button::new("ask-again").small().label("Ask again").on_click(cx.listener(|this, _, _, cx| {
                    this.asked_at = this.now();
                    cx.notify();
                })),
            )
            .child(div().text_xs().text_color(muted).child("Size"))
            .child(h_flex().gap_1().children(sizes))
            .child(
                Button::new("pause")
                    .small()
                    .label(if self.paused_at.is_some() { "Play" } else { "Pause" })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.paused_at = match this.paused_at {
                            Some(at) => {
                                this.start = Instant::now() - std::time::Duration::from_secs_f32(at);
                                None
                            }
                            None => Some(this.now()),
                        };
                        cx.notify();
                    })),
            )
    }

    fn lamp_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted) = (theme.border, theme.muted_foreground);
        let closed_red = CLOSED_RED.load(Ordering::Relaxed);
        let draft_own = DRAFT_OWN.load(Ordering::Relaxed);
        let toggle = |id: &'static str, label: &'static str, selected: bool, flag: &'static AtomicBool, value: bool| {
            Button::new(id).small().ghost().label(label).selected(selected).on_click(cx.listener(move |_, _, _, cx| {
                flag.store(value, Ordering::Relaxed);
                cx.notify();
            }))
        };
        h_flex()
            .flex_none()
            .w_full()
            .h(px(42.))
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(border)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Closed PR"))
            .child(
                h_flex()
                    .gap_1()
                    .child(toggle("closed-grey", "Grey, like no PR", !closed_red, &CLOSED_RED, false))
                    .child(toggle("closed-red", "GitHub's red", closed_red, &CLOSED_RED, true)),
            )
            .child(div().w(px(12.)))
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Draft PR"))
            .child(
                h_flex()
                    .gap_1()
                    .child(toggle("draft-same", "Same as no PR", !draft_own, &DRAFT_OWN, false))
                    .child(toggle("draft-own", "A grey of its own", draft_own, &DRAFT_OWN, true)),
            )
            .child(div().text_xs().text_color(muted).child("In the sidebar, holly is closed and beech is a draft. The grid on the right shows every PR state."))
    }

    fn sidebar(&self, t: f32, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted, selected_bg) =
            (theme.sidebar, theme.sidebar_border, theme.muted_foreground, theme.sidebar_accent);
        let (small, show) = (self.small, self.show(t));
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
                    let local = Show { t: t + offset + i as f32 * 0.37, ..show };
                    let (state, local, note) = match row.state {
                        Some(state) => (state, local, None),
                        None => {
                            let (state, since_start, note) = journey(t);
                            (state, Show { since: t - since_start, ..local }, Some(note))
                        }
                    };
                    let note = note.map_or_else(|| describe(state), str::to_string);
                    h_flex()
                        .h(px(30.))
                        .px_2()
                        .gap_2()
                        .rounded_md()
                        .text_sm()
                        .when(row.selected, |d| d.bg(selected_bg).font_weight(FontWeight::MEDIUM))
                        .child(div().w(px(small + 2.0)).flex().justify_center().child(mark(state, small, local)))
                        .child(div().flex_1().min_w_0().child(row.name))
                        .child(div().text_size(px(11.)).text_color(faint()).whitespace_nowrap().child(note))
                }))
        };
        v_flex()
            .flex_none()
            .w(px(330.))
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
                    Row { name: "alder", state: Some(state(Pr::None, Agent::Idle, You::Nothing)), selected: false },
                    Row { name: "birch", state: None, selected: false },
                    Row { name: "cedar", state: Some(state(Pr::Open, Agent::Idle, You::NeedsYou)), selected: false },
                    Row { name: "aspen", state: Some(state(Pr::Open, Agent::Working, You::Nothing)), selected: true },
                    Row { name: "beech", state: Some(state(Pr::Draft, Agent::Working, You::Nothing)), selected: false },
                ],
                0.0,
            ))
            .child(group(
                "signal-box",
                vec![
                    Row { name: "elm", state: Some(state(Pr::None, Agent::Idle, You::Failed)), selected: false },
                    Row { name: "fir", state: Some(state(Pr::Merged, Agent::Working, You::Nothing)), selected: false },
                    Row { name: "hazel", state: Some(state(Pr::None, Agent::SettingUp, You::Nothing)), selected: false },
                    Row { name: "juniper", state: Some(state(Pr::Merged, Agent::Idle, You::Nothing)), selected: false },
                    Row { name: "holly", state: Some(state(Pr::Closed, Agent::Idle, You::Nothing)), selected: false },
                    Row { name: "ivy", state: Some(state(Pr::Open, Agent::Idle, You::Failed)), selected: false },
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
                    .child("birch goes through a whole job, about every 30 seconds."),
            )
    }

    fn pane(&self, t: f32, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (border, muted, raised, surface, input, bg, fg) = (
            theme.border,
            theme.muted_foreground,
            theme.secondary,
            theme.muted,
            theme.input,
            theme.background,
            theme.foreground,
        );
        let show = self.show(t);
        let tiny = self.small - 2.0;
        let tab = |title: &'static str, state: State, selected: bool, offset: f32| {
            h_flex()
                .h(px(28.))
                .pl_2()
                .pr_3()
                .gap_1p5()
                .rounded_md()
                .text_size(px(13.))
                .when(selected, |d| d.bg(raised))
                .when(!selected, |d| d.text_color(muted))
                .child(mark(state, tiny, Show { t: t + offset, ..show }))
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
            .max_w(px(640.))
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
                    .border_color(amber().opacity(0.45))
                    .bg(amber().opacity(0.05))
                    .child(
                        h_flex()
                            .gap_2()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(amber())
                            .child(mark(state(Pr::Open, Agent::Idle, You::NeedsYou), tiny, show))
                            .child("Your turn · PR state"),
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
                    .child(mark(state(Pr::Open, Agent::Working, You::Nothing), self.small, Show { t: t + 0.2, ..show }))
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
                    .child(div().text_xs().font_family("Menlo").text_color(muted).child("sidebar-pr-state"))
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .h(px(26.))
                            .px_2()
                            .gap_1p5()
                            .rounded_md()
                            .border_1()
                            .border_color(border)
                            .text_xs()
                            .text_color(fg)
                            .child(div().size(px(7.)).rounded_full().bg(Pr::Open.color()))
                            .child("#182 · Open"),
                    ),
            )
            .child(
                h_flex()
                    .flex_none()
                    .h(px(40.))
                    .px_3()
                    .gap_1()
                    .border_b_1()
                    .border_color(border)
                    .child(tab("Wire PR state into the feed", state(Pr::Open, Agent::Working, You::Nothing), true, 0.5))
                    .child(tab("Check the CLI fixture", state(Pr::Open, Agent::Idle, You::NeedsYou), false, 0.0))
                    .child(tab("Chat 3", state(Pr::Open, Agent::Idle, You::Nothing), false, 0.0)),
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
                                        .text_color(fg)
                                        .child(mark(state(Pr::Open, Agent::Working, You::Nothing), tiny, Show { t: t + 0.4, ..show }))
                                        .child("Stop"),
                                ),
                        ),
                ),
            )
    }

    fn sheet(&self, t: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, border, muted, fg, raised, selected_bg) =
            (theme.sidebar, theme.border, theme.muted_foreground, theme.foreground, theme.secondary, theme.sidebar_accent);
        let show = self.show(t);
        let small = self.small;
        let picked = self.picked;
        let heading = |text: &'static str| div().text_xs().font_weight(FontWeight::MEDIUM).text_color(muted).child(text);
        let choice = |id: String, label: &'static str, selected: bool| {
            Button::new(ElementId::Name(id.into())).xsmall().ghost().label(label).selected(selected)
        };
        let pr_buttons: Vec<AnyElement> = PRS
            .iter()
            .map(|pr| {
                let pr = *pr;
                choice(format!("pick-pr-{}", pr.name()), pr.name(), picked.pr == pr)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.picked.pr = pr;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let agent_buttons: Vec<AnyElement> = AGENTS
            .iter()
            .map(|agent| {
                let agent = *agent;
                choice(format!("pick-agent-{}", agent.name()), agent.name(), picked.agent == agent)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.picked.agent = agent;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let you_buttons: Vec<AnyElement> = YOUS
            .iter()
            .map(|you| {
                let you = *you;
                choice(format!("pick-you-{}", you.name()), you.name(), picked.you == you)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.picked.you = you;
                        if you == You::NeedsYou {
                            this.asked_at = this.now();
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let picker_row = |label: &'static str, buttons: Vec<AnyElement>| {
            h_flex()
                .gap_2()
                .child(div().w(px(96.)).text_xs().text_color(muted).child(label))
                .child(h_flex().flex_wrap().gap_0p5().children(buttons))
        };
        let columns: [(Agent, You, &str); 5] = [
            (Agent::Idle, You::Nothing, "At rest"),
            (Agent::SettingUp, You::Nothing, "Setting up"),
            (Agent::Working, You::Nothing, "Working"),
            (Agent::Idle, You::NeedsYou, "Your turn"),
            (Agent::Idle, You::Failed, "Stopped short"),
        ];
        let grid_head = h_flex()
            .gap_1()
            .child(div().w(px(64.)))
            .children(columns.iter().map(|(_, _, label)| div().w(px(62.)).text_size(px(11.)).text_center().text_color(faint()).child(*label)));
        let grid_rows = PRS.iter().map(|pr| {
            let pr = *pr;
            h_flex()
                .gap_1()
                .py_1()
                .child(div().w(px(64.)).text_xs().text_color(muted).child(pr.name()))
                .children(columns.iter().map(move |(agent, you, _)| {
                    div().w(px(62.)).flex().justify_center().child(mark(state(pr, *agent, *you), 36.0, show))
                }))
        });
        let tile = |label: &'static str, body: AnyElement| {
            v_flex()
                .flex_1()
                .h(px(150.))
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
        let lamp_now = self.lamp;
        let lamp_cols: [(State, &str); 4] = [
            (state(Pr::Open, Agent::Idle, You::NeedsYou), "Open, your turn"),
            (state(Pr::Merged, Agent::Idle, You::NeedsYou), "Merged, your turn"),
            (state(Pr::None, Agent::Idle, You::Failed), "No PR, stopped"),
            (state(Pr::Open, Agent::Idle, You::Failed), "Open, stopped"),
        ];
        let lamp_head = h_flex()
            .gap_1()
            .child(div().w(px(104.)))
            .children(lamp_cols.iter().map(|(_, label)| div().w(px(68.)).text_size(px(11.)).text_center().text_color(faint()).child(*label)));
        let lamp_rows: Vec<AnyElement> = LAMPS
            .iter()
            .map(|(lamp, label)| {
                let lamp = *lamp;
                let selected = lamp == lamp_now;
                let show = Show { lamp, ..show };
                h_flex()
                    .id(ElementId::Name(format!("lamp-row-{label}").into()))
                    .gap_1()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |d| d.bg(selected_bg))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.lamp = lamp;
                        this.asked_at = this.now();
                        cx.notify();
                    }))
                    .child(div().w(px(104.)).pl_2().text_xs().text_color(if selected { fg } else { muted }).child(*label))
                    .children(lamp_cols.iter().map(move |(st, _)| {
                        v_flex()
                            .w(px(68.))
                            .items_center()
                            .gap_1()
                            .child(mark(*st, 40.0, show))
                            .child(mark(*st, small, show))
                    }))
                    .into_any_element()
            })
            .collect();
        v_flex()
            .id("sheet")
            .flex_none()
            .w(px(420.))
            .h_full()
            .overflow_y_scroll()
            .bg(bg)
            .border_l_1()
            .border_color(border)
            .p_4()
            .gap_4()
            .child(heading("Where amber and red go, five ways. Click one to use it everywhere"))
            .child(v_flex().child(lamp_head).children(lamp_rows))
            .child(heading("Try any combination"))
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .rounded_lg()
                    .bg(selected_bg)
                    .child(
                        h_flex()
                            .justify_center()
                            .gap_6()
                            .py_2()
                            .child(mark(picked, 120.0, show))
                            .child(mark(picked, small, show)),
                    )
                    .child(div().text_xs().text_center().text_color(fg).child(describe(picked)))
                    .child(picker_row("The work's PR", pr_buttons))
                    .child(picker_row("The agent", agent_buttons))
                    .child(picker_row("You", you_buttons)),
            )
            .child(heading("Every combination"))
            .child(v_flex().child(grid_head).children(grid_rows))
            .child(heading("Bigger moments, where it's the brand"))
            .child(
                h_flex()
                    .gap_3()
                    .child(tile(
                        "Empty state",
                        v_flex()
                            .items_center()
                            .gap_2()
                            .child(brand(48.0))
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
                            .gap_2()
                            .child(mark(state(Pr::None, Agent::Working, You::Nothing), 44.0, show))
                            .child(div().text_xs().text_color(muted).child("Connecting…"))
                            .into_any_element(),
                    ))
                    .child(tile(
                        "App icon",
                        div()
                            .size(px(76.))
                            .rounded(px(17.))
                            .border_1()
                            .border_color(border)
                            .bg(raised)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(brand(48.0))
                            .into_any_element(),
                    )),
            )
    }
}

fn describe(state: State) -> String {
    let work = match state.pr {
        Pr::None => "no PR",
        Pr::Draft => "draft PR",
        Pr::Open => "PR open",
        Pr::Merged => "merged",
        Pr::Closed => "PR closed",
    };
    let agent = match state.agent {
        Agent::Idle => None,
        Agent::SettingUp => Some("setting up"),
        Agent::Working => Some("working"),
    };
    let you = match state.you {
        You::Nothing => None,
        You::NeedsYou => Some("your turn"),
        You::Failed => Some("stopped short"),
    };
    [Some(work), agent, you].into_iter().flatten().collect::<Vec<_>>().join(", ")
}

impl Render for Framework {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.now();
        let theme = cx.theme();
        let (bg, fg) = (theme.background, theme.foreground);
        let toolbar = self.toolbar(cx).into_any_element();
        let lamp_bar = self.lamp_bar(cx).into_any_element();
        let sidebar = self.sidebar(t, cx).into_any_element();
        let pane = self.pane(t, cx).into_any_element();
        let sheet = self.sheet(t, cx).into_any_element();
        if self.fixed.is_none() && self.paused_at.is_none() {
            window.request_animation_frame();
        }
        v_flex()
            .id("framework")
            .size_full()
            .bg(bg)
            .text_color(fg)
            .child(toolbar)
            .child(lamp_bar)
            .child(h_flex().flex_1().min_h_0().w_full().child(sidebar).child(pane).child(sheet))
    }
}
