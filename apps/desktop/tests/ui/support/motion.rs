//! Playing the frames the app asks for, and what moves in it. A test's window draws when something in it changes, and
//! not again for something that only asked for the next frame, such as a list measuring a few more rows, so a test
//! plays those. A test's clock stands still too, so the driver opens the app with Reduce Motion on, and a test of
//! something that moves turns motion back on and plays it from here.

#![cfg(test)]

use std::time::Duration;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt as _;

use super::Ui;

/// More frames than anything that ends could ask for one after another.
const SETTLES_WITHIN: usize = 500;

/// How long a frame is, on a display that draws 60 a second.
pub(super) const A_FRAME: Duration = Duration::from_nanos(16_666_667);

impl Ui<'_> {
    /// Brings the window's next frame, as a display would. A test has no display, so what waits for the next frame
    /// otherwise waits until something else is drawn, however much later that is. What the frame brings shows once
    /// the app has caught up, as after [`Ui::frame`]. Returns how many things were waiting for it.
    pub(crate) fn bring_the_next_frame(&mut self) -> usize {
        self.cx.update_window(self.handle, |_, window, cx| window.simulate_next_frame(cx)).unwrap()
    }

    /// Presses each of `keys` a frame apart, as someone does while the app moves something a step a frame: each key's
    /// down arrives between two frames, a frame's time after the one before, and the next frame draws what it did
    /// along with the step, as a display brings it. [`Ui::press`] draws a frame before a key and after it, so a key
    /// pressed with it never lands partway through. They're let go once the last is drawn, since GPUI draws a window
    /// that changed before it takes a key, which would add a frame between them.
    pub(crate) fn press_a_frame_apart(&mut self, keys: &[&str]) {
        let mut held = Vec::new();
        for key in keys {
            self.cx.executor().advance_clock(A_FRAME);
            self.frame();
            let down = self.cx.update_window(self.handle, |_, window, cx| {
                let down = window.between_frames().key_down(key, cx);
                window.simulate_next_frame(cx);
                down
            });
            held.push(down.unwrap());
        }
        self.frame();
        let let_go = |_, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App| {
            for key in held {
                window.between_frames().key_up(key, cx);
            }
        };
        self.cx.update_window(self.handle, let_go).unwrap();
    }

    /// Lets things move, as they do for someone whose Mac doesn't reduce motion.
    pub(crate) fn allow_motion(&mut self) {
        self.cx.update(|cx| cx.set_reduce_motion(false));
    }

    /// Lets `time` pass on the app's clock, and draws the frame that whatever asked for one is waiting on. Returns how
    /// many things had asked, which is none once everything has come to rest.
    pub(crate) fn play(&mut self, time: Duration) -> usize {
        self.frame();
        self.cx.executor().advance_clock(time);
        let asked = self.bring_the_next_frame();
        self.frame();
        asked
    }

    /// Plays frames, `step` apart, until nothing asks for another, so what's checked next is the app at rest. Panics if
    /// something never stops asking, which would keep an idle window drawing.
    ///
    /// It returns straight after finding nothing waiting, without letting the app run again. A page of history
    /// arrives from another thread whenever it does, and one taken up after the last look would leave the app with
    /// work it hadn't asked a frame for yet, which is not at rest.
    pub(crate) fn settle(&mut self, step: Duration) {
        for _ in 0..SETTLES_WITHIN {
            // Whatever has arrived is taken up and drawn last, so that nothing runs between that and the look at
            // what's waiting.
            self.cx.executor().advance_clock(step);
            self.frame();
            if self.bring_the_next_frame() == 0 {
                return;
            }
        }
        panic!("something is still asking for frames after {SETTLES_WITHIN} of them");
    }
}
