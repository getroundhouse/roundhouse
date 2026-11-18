//! A status mark, or a background task's ring, asks for the display's next frame only while it's moving, so it moves as
//! smoothly as the display can show, and a window of still ones draws nothing.

#![cfg(test)]

use std::sync::Arc;

use gpui_kit::assets::Assets;
use gpui_kit::component::h_flex;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyElement, AppContext as _, Bounds, Context, HeadlessAppContext, IntoElement, ParentElement, PlatformTextSystem,
    Render, Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use look::{Moving, MovingMarks, Signal, TaskRing, Work};

#[derive(Clone, Copy)]
enum Drawn {
    Mark(Signal),
    Ring(TaskRing),
    /// Room for a mark that moves, which is painted over the view rather than in it.
    Room(Moving),
}

struct Marks {
    drawn: Vec<Drawn>,
    moving: MovingMarks,
}

impl Render for Marks {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.moving.draw_again();
        let moving = self.moving.clone();
        let element = |drawn: &Drawn| -> AnyElement {
            match *drawn {
                Drawn::Mark(signal) => look::mark(signal, Work::Open, look::Size::Row, cx).into_any_element(),
                Drawn::Ring(ring) => look::task_ring(ring, cx).into_any_element(),
                Drawn::Room(mark) => moving.room(mark).into_any_element(),
            }
        };
        div().relative().child(h_flex().children(self.drawn.iter().map(element))).child(self.moving.over())
    }
}

#[derive(Clone, Copy)]
enum Motion {
    Allowed,
    Reduced,
    HeldStill,
}

/// How many callbacks the window has waiting for its next frame, after drawing marks with `signals` once.
fn frames_asked_for(signals: &[Signal], motion: Motion) -> usize {
    frames_for(signals.iter().copied().map(Drawn::Mark).collect(), motion)
}

fn frames_for(drawn: Vec<Drawn>, motion: Motion) -> usize {
    let text_system: Arc<dyn PlatformTextSystem> = Arc::new(gpui_apple::AppleTextSystem::new());
    let mut cx =
        HeadlessAppContext::with_platform(text_system, Arc::new(Assets), gpui_kit::platform::current_headless_renderer);
    cx.update(gpui_kit::init);
    cx.update(look::apply);
    match motion {
        Motion::Allowed => cx.update(|cx| cx.set_reduce_motion(false)),
        Motion::Reduced => cx.update(|cx| cx.set_reduce_motion(true)),
        Motion::HeldStill => cx.update(|cx| look::hold_still(0, cx)),
    }
    let options = |cx: &mut gpui_kit::App| WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size(px(200.), px(40.)), cx))),
        focus: false,
        show: false,
        ..Default::default()
    };
    let marks = |_: &mut Context<Marks>| Marks { drawn, moving: MovingMarks::default() };
    let (handle, _) = cx.update(|cx| gpui_kit::open_window(options(cx), cx, |_, cx| cx.new(marks))).unwrap();
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
    assert!(frames_asked_for(&[Signal::Background], Motion::Allowed) > 0);
}

#[test]
fn a_running_tasks_ring_moves_and_a_monitors_holds_still() {
    assert!(frames_for(vec![Drawn::Ring(TaskRing::Running)], Motion::Allowed) > 0);
    assert_eq!(frames_for(vec![Drawn::Ring(TaskRing::Watching)], Motion::Allowed), 0);
}

#[test]
fn still_marks_ask_for_nothing() {
    assert_eq!(frames_asked_for(&[Signal::Idle, Signal::Failed], Motion::Allowed), 0);
}

#[test]
fn reduce_motion_and_pictures_hold_a_working_mark_still() {
    assert_eq!(frames_asked_for(&[Signal::Working, Signal::Background], Motion::Reduced), 0);
    assert_eq!(frames_asked_for(&[Signal::Working, Signal::Background], Motion::HeldStill), 0);
    assert_eq!(frames_for(vec![Drawn::Ring(TaskRing::Running)], Motion::Reduced), 0);
    assert_eq!(frames_for(vec![Drawn::Ring(TaskRing::Running)], Motion::HeldStill), 0);
}

#[test]
fn a_moving_mark_painted_over_its_view_asks_for_the_next_frame_and_a_still_one_doesnt() {
    let rooms = |marks: &[Moving]| marks.iter().copied().map(Drawn::Room).collect::<Vec<_>>();
    let moving = [Moving::ChatMark(Signal::Working), Moving::TaskRing(TaskRing::Running), Moving::Robot];

    for mark in moving {
        assert!(frames_for(rooms(&[mark]), Motion::Allowed) > 0, "{mark:?} held still");
        assert_eq!(frames_for(rooms(&[mark]), Motion::Reduced), 0, "{mark:?} moved under Reduce Motion");
    }
    let still = [Moving::ChatMark(Signal::Idle), Moving::TaskRing(TaskRing::Watching)];
    assert_eq!(frames_for(rooms(&still), Motion::Allowed), 0);
    assert_eq!(frames_for(Vec::new(), Motion::Allowed), 0, "with no room left, nothing is painted over");
}
