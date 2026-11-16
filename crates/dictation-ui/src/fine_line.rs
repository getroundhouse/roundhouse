//! A quiet, voice-reactive boundary. Its own entity receives levels and redraws, leaving the host's text and controls
//! out of the animation. Silence and Reduce Motion leave a still line and request no frames.

use std::f32::consts::PI;
use std::time::Instant;

use gpui_kit::{
    App, BorderStyle, Bounds, Context, Corners, Edges, Hsla, IntoElement, PathBuilder, Pixels, Render, Styled as _,
    Task, Window, canvas, linear_color_stop, linear_gradient, point, px, quad, transparent_black,
};
use look::motion::Sliding;

pub(crate) struct FineLine {
    visibility: Sliding,
    energy: Sliding,
    since: Instant,
    corner: look::Corner,
    levels: Option<Task<()>>,
}

impl FineLine {
    pub(crate) fn new(corner: look::Corner, cx: &App) -> Self {
        Self {
            visibility: Sliding::at(0.),
            energy: Sliding::at(0.),
            since: cx.background_executor().now(),
            corner,
            levels: None,
        }
    }

    pub(crate) fn listen(&mut self, levels: flume::Receiver<dictation::Level>, cx: &mut Context<Self>) {
        self.since = cx.background_executor().now();
        self.visibility.go(1., cx);
        self.levels = Some(cx.spawn(async move |this, cx| {
            while let Ok(level) = levels.recv_async().await {
                if this
                    .update(cx, |line, cx| {
                        line.energy.go(level.strength(), cx);
                        if !cx.reduce_motion() {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        cx.notify();
    }

    pub(crate) fn settle(&mut self, cx: &mut Context<Self>) {
        self.levels = None;
        self.energy.go(0., cx);
        cx.notify();
    }

    pub(crate) fn hide(&mut self, cx: &mut Context<Self>) {
        self.settle(cx);
        self.visibility.go(0., cx);
    }
}

impl Render for FineLine {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let opacity = self.visibility.draw(window, cx);
        let energy = if cx.reduce_motion() { 0. } else { self.energy.draw(window, cx) };
        let time = cx.background_executor().now().saturating_duration_since(self.since).as_secs_f32();
        let (color, corner) = (look::colors(cx).brand, self.corner);
        canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                if opacity > 0. {
                    paint(bounds, color, corner, opacity, energy, time, window);
                    if energy > 0. {
                        window.request_animation_frame();
                    }
                }
            },
        )
        .size_full()
    }
}

fn paint(
    bounds: Bounds<Pixels>,
    color: Hsla,
    corner: look::Corner,
    opacity: f32,
    energy: f32,
    time: f32,
    window: &mut Window,
) {
    window.paint_quad(quad(
        bounds,
        Corners::all(corner.radius()),
        transparent_black(),
        Edges::all(look::EDGE),
        color.opacity(opacity * 0.14),
        BorderStyle::default(),
    ));
    let margin = look::Space::Medium.size();
    let width = (bounds.size.width - margin * 2.).max(px(0.));
    let y = bounds.bottom() - look::EDGE * 1.8;
    let height = if bounds.size.height < px(140.) { 2.5 } else { 3.5 };
    // Two joined strokes give the hairline transparent ends, without a glow or a highlighted moving point.
    for half in 0..2_u16 {
        let mut path = PathBuilder::stroke(look::EDGE);
        for step in 0..=50_u16 {
            let u = f32::from(half * 50 + step) / 100.;
            let envelope = (u * PI).sin().powi(2);
            let ripple = 0.68 + 0.32 * (u * 7. - time * 2.2).sin();
            let at = point(bounds.left() + margin + width * u, y - px(height * energy * envelope * ripple));
            if step == 0 {
                path.move_to(at);
            } else {
                path.line_to(at);
            }
        }
        if let Ok(path) = path.build() {
            let ink = color.opacity(opacity * 0.7);
            let clear = color.opacity(0.);
            let (from, to) = if half == 0 { (clear, ink) } else { (ink, clear) };
            window.paint_path(path, linear_gradient(90., linear_color_stop(from, 0.), linear_color_stop(to, 1.)));
        }
    }
}
