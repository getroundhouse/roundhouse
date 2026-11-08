//! SKETCH, never merged: the fan of stalls in every state, through one cycle of its animation, as frames in
//! `.context/motif/` for a GIF.

#![cfg(test)]

use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::assets::Assets;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, HeadlessAppContext, IntoElement, ParentElement, PlatformTextSystem, Render,
    Styled, Window, WindowBounds, WindowOptions, div, px, size,
};
use transcript::look::{self, Signal, Work};

const FRAMES: usize = 96;

struct Sheet {
    phase: f32,
}

impl Render for Sheet {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg, fg, muted, border) = (theme.background, theme.foreground, theme.muted_foreground, theme.border);
        let signals = [
            (Signal::Idle, "Not running"),
            (Signal::SettingUp, "Setting up"),
            (Signal::Working, "Working"),
            (Signal::NeedsYou, "Your turn"),
            (Signal::Failed, "Stopped short"),
        ];
        let works = [(Work::NoPr, "No PR"), (Work::Open, "PR open"), (Work::Merged, "Merged")];
        let phase = self.phase;
        let head = h_flex()
            .child(div().w(px(110.)))
            .children(signals.iter().map(|(_, name)| div().w(px(150.)).text_sm().text_center().text_color(fg).child(*name)));
        let rows = works.iter().map(|(work, label)| {
            h_flex()
                .py_2()
                .child(div().w(px(110.)).text_sm().text_color(muted).child(*label))
                .children(signals.iter().map(|(signal, _)| {
                    h_flex()
                        .w(px(150.))
                        .justify_center()
                        .items_end()
                        .gap_3()
                        .child(look::fan_at(*signal, *work, px(84.), phase, cx))
                        .child(look::fan_at(*signal, *work, px(18.), phase, cx))
                }))
        });
        v_flex()
            .size_full()
            .bg(bg)
            .p_8()
            .gap_5()
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_base().text_color(fg).child("The stalls, in three layers"))
                    .child(div().text_sm().text_color(muted).child(
                        "The fan's colour is the work's PR. A white light over it is the agent. When it's your turn, \
                         or the agent stopped short, the whole fan turns amber or red.",
                    )),
            )
            .child(v_flex().pt_5().border_t_1().border_color(border).child(head).children(rows))
    }
}

#[test]
fn draw_the_fan_through_one_cycle() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/motif");
    std::fs::create_dir_all(&out).unwrap();
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
    cx.update(gpui_kit::init);
    let (handle, view) = cx
        .update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(990.), px(470.)), cx))),
                focus: false,
                show: false,
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(window), cx);
                desktop::apply_theme(Some(window), cx);
                cx.new(|_| Sheet { phase: 0.0 })
            })
        })
        .unwrap();
    for frame in 0..FRAMES {
        let phase = frame as f32 / FRAMES as f32 * look::CYCLE;
        cx.update(|cx| view.update(cx, |s, cx| {
            s.phase = phase;
            cx.notify();
        }));
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
        let image = cx.capture_screenshot(handle).unwrap();
        image.save(out.join(format!("frame-{frame:02}.png"))).unwrap();
    }
}

#[path = "../../examples/motifs/view.rs"]
#[allow(dead_code)]
mod motifs_view;

/// SKETCH: the motif app at a few moments, for checking by eye before opening it.
#[test]
fn draw_the_motif_app() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/motif-app");
    std::fs::create_dir_all(&out).unwrap();
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
    cx.update(gpui_kit::init);
    let (handle, view) = cx
        .update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1520.), px(900.)), cx))),
                focus: false,
                show: false,
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(window), cx);
                desktop::apply_theme(Some(window), cx);
                cx.new(|_| {
                    let mut m = motifs_view::Motifs::new();
                    m.fixed = Some(4.3);
                    m
                })
            })
        })
        .unwrap();
    for (i, kind) in motifs_view::KINDS.iter().copied().enumerate().filter(|(i, _)| *i == 6) {
        for t in [4.3_f32] {
            cx.update(|cx| view.update(cx, |m, cx| {
                m.kind = kind;
                m.fixed = Some(t);
                cx.notify();
            }));
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
            let image = cx.capture_screenshot(handle).unwrap();
            image.save(out.join(format!("kind-{i}.png"))).unwrap();
        }
    }
}

#[path = "../../examples/motifs/framework.rs"]
#[allow(dead_code)]
mod framework_view;

/// SKETCH: the three-layer framework at one moment, for checking by eye before opening it.
#[test]
fn draw_the_framework() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.context/motif-app");
    std::fs::create_dir_all(&out).unwrap();
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
    cx.update(gpui_kit::init);
    let (handle, _view) = cx
        .update(|cx| {
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(1560.), px(920.)), cx))),
                focus: false,
                show: false,
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, Some(window), cx);
                desktop::apply_theme(Some(window), cx);
                cx.new(|_| {
                    let mut f = framework_view::Framework::new();
                    f.fixed = Some(5.0);
                    f
                })
            })
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx)).unwrap();
    cx.capture_screenshot(handle).unwrap().save(out.join("framework.png")).unwrap();
}
