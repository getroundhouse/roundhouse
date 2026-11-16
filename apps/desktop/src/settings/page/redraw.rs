//! What a page of its own keeps to draw itself again when what it says of a time next changes, such as a login's end,
//! with nothing else changed that would draw it, and to bring a panel into sight as it opens.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{Bounds, Context, Pixels, ScrollHandle, Task, point, px};

/// Draws the page whose state keeps it again at a time by the wall clock.
#[derive(Default)]
pub(super) struct RedrawAt {
    /// When it's to, until it has.
    at_ms: Rc<Cell<Option<i64>>>,
    waiting: Option<Task<()>>,
}

impl RedrawAt {
    /// Draws the page again at `at_ms`, or never, in place of whenever it was to before. Once it has, the same time
    /// sets it again, as when the clock fired a moment before what it waited for, by the wall clock.
    pub(super) fn set<T: 'static>(&mut self, at_ms: Option<i64>, cx: &mut Context<T>) {
        if self.at_ms.get() == at_ms {
            return;
        }
        self.at_ms.set(at_ms);
        self.waiting = at_ms.map(|at_ms| {
            let wait = Duration::from_millis(at_ms.saturating_sub(look::now_ms(cx)).max(0).unsigned_abs());
            let fired = self.at_ms.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                fired.set(None);
                let _ = this.update(cx, |_, cx| cx.notify());
            })
        });
    }
}

/// Scrolls `scroll` the least that brings `bounds`, where something on it was just laid out, into sight, or as much
/// of it as fits, from its top. Says whether it had to.
pub(super) fn bring_into_sight(scroll: &ScrollHandle, bounds: Bounds<Pixels>) -> bool {
    let sight = scroll.bounds();
    let by = if bounds.top() < sight.top() || bounds.size.height > sight.size.height {
        sight.top() - bounds.top()
    } else if bounds.bottom() > sight.bottom() {
        sight.bottom() - bounds.bottom()
    } else {
        return false;
    };
    let offset = scroll.offset();
    scroll.set_offset(point(offset.x, (offset.y + by).min(px(0.))));
    true
}
