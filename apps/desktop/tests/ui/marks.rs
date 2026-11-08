//! A status mark asks for the display's next frame only while it's moving, so it moves as smoothly as the display
//! can show, and a window of still marks draws nothing.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::assets::Assets;
use gpui_kit::component::h_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AppContext as _, Bounds, Context, HeadlessAppContext, IntoElement, ParentElement, PlatformTextSystem, Render,
    Window, WindowBounds, WindowOptions, px, size,
};
use look::{Signal, Work};

struct Marks(Vec<Signal>);

impl Render for Marks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex().children(self.0.iter().map(|signal| look::mark(*signal, Work::Open, look::Size::Row, cx)))
    }
}

#[derive(Clone, Copy)]
enum Motion {
    Allowed,
    Reduced,
    HeldStill,
}

/// How many callbacks the window has waiting for its next frame, after drawing `signals` once.
fn frames_asked_for(signals: &[Signal], motion: Motion) -> usize {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
    cx.update(gpui_kit::init);
    match motion {
        Motion::Allowed => {}
        Motion::Reduced => cx.update(|cx| cx.set_reduce_motion(true)),
        Motion::HeldStill => cx.update(look::hold_still),
    }
    let signals = signals.to_vec();
    let options = |cx: &mut gpui_kit::App| WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(200.), px(40.)), cx))),
        focus: false,
        show: false,
        ..Default::default()
    };
    let (handle, _) =
        cx.update(|cx| gpui_kit::open_window(options(cx), cx, |_, cx| cx.new(|_| Marks(signals)))).unwrap();
    cx.run_until_parked();
    cx.update_window(handle, |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
        window.simulate_next_frame(cx)
    })
    .unwrap()
}

#[test]
fn a_moving_mark_asks_for_the_next_frame() {
    assert!(frames_asked_for(&[Signal::Working], Motion::Allowed) > 0);
    assert!(frames_asked_for(&[Signal::SettingUp], Motion::Allowed) > 0);
}

#[test]
fn still_marks_ask_for_nothing() {
    assert_eq!(frames_asked_for(&[Signal::Idle, Signal::NeedsYou, Signal::Failed], Motion::Allowed), 0);
}

#[test]
fn reduce_motion_and_pictures_hold_a_working_mark_still() {
    assert_eq!(frames_asked_for(&[Signal::Working], Motion::Reduced), 0);
    assert_eq!(frames_asked_for(&[Signal::Working], Motion::HeldStill), 0);
}
