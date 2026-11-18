//! Marks that move, in a view that's cached. A moving mark asks for each frame from the view it's painted in, which
//! then draws everything in it again, so a mark inside a cached view would have all of that view drawn on every frame,
//! and one the view's cache painted would stop moving. So a cached view leaves room for each of its moving marks, which
//! notes where it was painted, and the view around it, which isn't cached, paints the marks there, over it. Then a
//! mark's frames draw the view around again, and the cached view is painted from its cache.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::{App, Bounds, ContentMask, IntoElement, Pixels, Styled, Window, canvas};

use crate::mark::{self, Signal, Size};
use crate::robot;
use crate::task_ring::{self, TaskRing};

/// A mark that moves for as long as what it's for is at work.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Moving {
    /// A chat's mark beside text, as [`crate::chat_mark`] draws it.
    ChatMark(Signal),
    /// A background task's ring, as [`crate::task_ring`] draws it.
    TaskRing(TaskRing),
    /// A subagent at work in the background, as [`crate::robot_at_work`] draws it.
    Robot,
}

impl Moving {
    /// Paints it within `bounds`, and returns whether it moves.
    fn paint(self, bounds: Bounds<Pixels>, window: &mut Window, cx: &App) -> bool {
        match self {
            Moving::ChatMark(signal) => mark::paint_chat_mark(signal, bounds, window, cx),
            Moving::TaskRing(ring) => task_ring::paint_task_ring(ring, bounds, window, cx),
            Moving::Robot => robot::paint_robot(bounds, window, cx),
        }
    }

    /// `room`, as big as the mark is drawn.
    fn sized<E: Styled>(self, room: E) -> E {
        match self {
            Moving::ChatMark(_) => mark::sized(room, Size::Inline.width()),
            Moving::TaskRing(_) | Moving::Robot => task_ring::sized(room),
        }
    }
}

/// Where a cached view's moving marks were painted when it was last drawn, which the view notes as it leaves room for
/// them, with [`MovingMarks::room`], and the view around it paints them from, with [`MovingMarks::over`].
#[derive(Clone, Default)]
pub struct MovingMarks(Rc<RefCell<Vec<Room>>>);

/// Where a mark was painted, and as much of it as showed.
struct Room {
    mark: Moving,
    bounds: Bounds<Pixels>,
    shown: ContentMask<Pixels>,
}

impl MovingMarks {
    /// Forgets where the marks were, as the cached view draws again, so only those it leaves room for this time are
    /// painted. A view painted from its cache leaves room for none, and its marks stay where they were.
    pub fn draw_again(&self) {
        self.0.borrow_mut().clear();
    }

    /// Room for `mark`, as big as it's drawn, where [`MovingMarks::over`] paints it.
    pub fn room(&self, mark: Moving) -> impl IntoElement {
        let rooms = self.0.clone();
        let room = canvas(
            |_, _, _| {},
            move |bounds, (), window, _| {
                rooms.borrow_mut().push(Room { mark, bounds, shown: window.content_mask() });
            },
        );
        mark.sized(room)
    }

    /// The marks, each painted where its room was, and only as much of it as showed. It goes after the cached view,
    /// over the same place and as big as it is, in the view around, so a mark asks that view for the next frame
    /// while it moves.
    pub fn over(&self) -> impl IntoElement {
        let rooms = self.0.clone();
        canvas(
            |_, _, _| {},
            move |_, (), window, cx| {
                let mut moves = false;
                for room in rooms.borrow().iter() {
                    let shown = Some(room.shown);
                    moves |= window.with_content_mask(shown, |window| room.mark.paint(room.bounds, window, cx));
                }
                if moves {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}
