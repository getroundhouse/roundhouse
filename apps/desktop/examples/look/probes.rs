//! Scrolling a real window's transcript the way a person would, printing where the list and its scrollbar are after
//! every step. A test window draws everything there is to draw between events, and a real one draws when it's ready,
//! so these show what a test can't, such as a row whose text is still being read when it scrolls into view.

use std::time::{Duration, Instant};

use desktop::app::AppView;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AsyncApp, Entity, InputEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, ScrollDelta, ScrollEvent, point, px,
};
use transcript::Transcript;

/// How often a trackpad sends an event, at 60 Hz.
const TICK: Duration = Duration::from_millis(16);
/// Long enough for the window to be drawn and the tail to load.
const SETTLE: Duration = Duration::from_secs(2);

pub(crate) enum Probe {
    /// `scroll:PX`: the wheel, by PX every tick, up to the start and back down to the end.
    Scroll(f32),
    /// `drag:PX`: the scrollbar's thumb, from the bottom of its track to the top, by PX every tick.
    Drag(f32),
}

impl Probe {
    pub(crate) fn parse(arg: &str) -> Option<Self> {
        let pixels = |text: &str| text.parse().expect("a number of pixels");
        arg.strip_prefix("scroll:")
            .map(|step| Self::Scroll(pixels(step)))
            .or_else(|| arg.strip_prefix("drag:").map(|step| Self::Drag(pixels(step))))
    }

    /// Prints a line per step: `PROBE`, milliseconds, the pointer's height when dragging, the row at the top and how
    /// far into it, and the thumb's place and the session's height as the scrollbar has them. Then `PROBE done`.
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
        }
        eprintln!("PROBE done");
    }
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
    let (top, (fraction, height)) =
        transcript.read_with(cx, |t, _| (t.scroller().logical_scroll_top(), t.scroller().thumb()));
    eprintln!(
        "PROBE {} {:.0} {} {:.1} {:.5} {:.0}",
        start.elapsed().as_millis(),
        pointer.map_or(-1., f32::from),
        top.item_ix,
        f32::from(top.offset_in_item),
        fraction,
        f32::from(height)
    );
    (fraction, (top.item_ix, top.offset_in_item))
}
