//! Scrolling a real window's transcript the way a person would, printing where the list and its scrollbar are after
//! every step, or jumping to a prompt from the outline and printing where the list lands. A test window draws
//! everything there is to draw between events, and a real one draws when it's ready, so these show what a test can't,
//! such as a row whose text is still being read when it scrolls into view.

use std::time::{Duration, Instant};

use desktop::app::AppView;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AsyncApp, ElementId, Entity, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, ScrollEvent, SharedString, point, px,
};
use protocol::model::seq_index;
use transcript::Transcript;

/// How often a trackpad sends an event, at 60 Hz.
const TICK: Duration = Duration::from_millis(16);
/// Long enough for the window to be drawn and the tail to load.
const SETTLE: Duration = Duration::from_secs(2);
/// Longer than a jump takes to be drawn.
const A_MOMENT: Duration = Duration::from_millis(500);

pub(crate) enum Probe {
    /// `scroll:PX`: the wheel, by PX every tick, up to the start and back down to the end.
    Scroll(f32),
    /// `drag:PX`: the scrollbar's thumb, from the bottom of its track to the top, by PX every tick.
    Drag(f32),
    /// `jump:K`: from the end of the chat, the pointer on the outline's ticks, and a click on the Kth prompt in the
    /// list they open, counting from 1. It has to be one the list shows.
    Jump(usize),
}

impl Probe {
    pub(crate) fn parse(arg: &str) -> Option<Self> {
        let pixels = |text: &str| text.parse().expect("a number of pixels");
        arg.strip_prefix("scroll:")
            .map(|step| Self::Scroll(pixels(step)))
            .or_else(|| arg.strip_prefix("drag:").map(|step| Self::Drag(pixels(step))))
            .or_else(|| arg.strip_prefix("jump:").map(|k| Self::Jump(k.parse().expect("a prompt's number"))))
    }

    /// Prints a line per step: `PROBE`, milliseconds, the pointer's height when dragging, the row at the top and how
    /// far into it, and the thumb's place and the session's height as the scrollbar has them. Then `PROBE done`. A
    /// jump prints that line before and after, and the row the prompt is in.
    pub(crate) async fn run(self, handle: &AnyWindowHandle, view: &Entity<AppView>, cx: &mut AsyncApp) {
        let transcript =
            view.read_with(cx, |v: &AppView, cx| v.panes[v.selected.as_ref().unwrap()].read(cx).transcript().unwrap());
        cx.background_executor().timer(SETTLE).await;
        let bounds = handle.update(cx, |_, window, _| window.find("transcript").bounds()).unwrap();
        let start = Instant::now();
        match self {
            Self::Scroll(step) => {
                for direction in [1., -1.] {
                    loop {
                        dispatch(handle, cx, wheel(bounds.center(), step * direction));
                        cx.background_executor().timer(TICK).await;
                        let (fraction, top) = report(&transcript, start, None, cx);
                        if (direction > 0. && top == (0, px(0.))) || (direction < 0. && fraction >= 1.) {
                            break;
                        }
                    }
                }
            }
            Self::Drag(step) => {
                // The bar shows once the list moves, with the thumb at the bottom of its track.
                dispatch(handle, cx, wheel(bounds.center(), 10.));
                cx.background_executor().timer(Duration::from_millis(50)).await;
                let mut at = point(bounds.right() - px(8.), bounds.bottom() - px(20.));
                dispatch(
                    handle,
                    cx,
                    MouseMoveEvent { position: at, pressed_button: None, modifiers: Modifiers::default() },
                );
                dispatch(handle, cx, press(at));
                while at.y > bounds.top() {
                    at.y -= px(step);
                    let held = Some(MouseButton::Left);
                    dispatch(
                        handle,
                        cx,
                        MouseMoveEvent { position: at, pressed_button: held, modifiers: Modifiers::default() },
                    );
                    cx.background_executor().timer(TICK).await;
                    report(&transcript, start, Some(at.y), cx);
                }
                let up = MouseUpEvent {
                    button: MouseButton::Left,
                    position: at,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                };
                dispatch(handle, cx, up);
            }
            Self::Jump(prompt) => jump(handle, view, &transcript, prompt, start, cx).await,
        }
        eprintln!("PROBE done");
    }
}

async fn jump(
    handle: &AnyWindowHandle,
    view: &Entity<AppView>,
    transcript: &Entity<Transcript>,
    prompt: usize,
    start: Instant,
    cx: &mut AsyncApp,
) {
    let client = view.read_with(cx, |v: &AppView, _| v.client().unwrap());
    let session = transcript.read_with(cx, |t, _| t.session_id().clone());
    // Off the main thread, so the window goes on drawing while the hub answers.
    let lines = cx.background_executor().spawn(async move { client.session_prompt_list(&session) }).await.unwrap();
    let row = seq_index(lines[prompt - 1].seq()) - 1;
    eprintln!("PROBE prompt {prompt} of {} is in row {row}", lines.len());
    report(transcript, start, None, cx);
    let ticks = handle.update(cx, |_, window, _| window.find("outline").bounds()).unwrap();
    point_at(handle, cx, ticks.center());
    let id = ElementId::from(SharedString::from(format!("outline-prompt/{row}")));
    let deadline = Instant::now() + SETTLE;
    let mut found = None;
    while found.is_none() && Instant::now() < deadline {
        cx.background_executor().timer(TICK).await;
        found = handle.update(cx, |_, window, _| window.try_find(id.clone()).map(|row| row.bounds())).unwrap();
    }
    let Some(bounds) = found else {
        eprintln!("PROBE the list doesn't show prompt {prompt}");
        return;
    };
    point_at(handle, cx, bounds.center());
    dispatch(handle, cx, press(bounds.center()));
    let up = MouseUpEvent {
        button: MouseButton::Left,
        position: bounds.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
    };
    dispatch(handle, cx, up);
    cx.background_executor().timer(A_MOMENT).await;
    report(transcript, start, None, cx);
}

fn point_at(handle: &AnyWindowHandle, cx: &mut AsyncApp, position: Point<Pixels>) {
    dispatch(handle, cx, MouseMoveEvent { position, pressed_button: None, modifiers: Modifiers::default() });
}

fn wheel(position: Point<Pixels>, up: f32) -> ScrollEvent {
    ScrollEvent { position, delta: ScrollDelta::Pixels(point(px(0.), px(up))), ..Default::default() }
}

fn press(position: Point<Pixels>) -> MouseDownEvent {
    MouseDownEvent {
        button: MouseButton::Left,
        position,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    }
}

fn dispatch(handle: &AnyWindowHandle, cx: &mut AsyncApp, event: impl InputEvent) {
    let _ = handle.update(cx, |_, window, cx| {
        window.dispatch_event(event.to_platform_input(), cx);
    });
}

/// Prints where things are, and returns the thumb's place and the row at the top with how far into it.
fn report(
    transcript: &Entity<Transcript>,
    start: Instant,
    pointer: Option<Pixels>,
    cx: &mut AsyncApp,
) -> (f32, (usize, Pixels)) {
    let (top, (fraction, height)) = transcript.read_with(cx, |t, _| (t.list().top(), t.list().thumb()));
    eprintln!(
        "PROBE {} {:.0} {} {:.1} {:.5} {:.0}",
        start.elapsed().as_millis(),
        pointer.map_or(-1., f32::from),
        top.row,
        f32::from(top.offset),
        fraction,
        f32::from(height)
    );
    (fraction, (top.row, top.offset))
}
