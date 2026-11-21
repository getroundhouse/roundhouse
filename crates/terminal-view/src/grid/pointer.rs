//! What the pointer does on the grid: a left press, the drag after it, even past the grid's edges, and the release,
//! each given to client-core's copy of the terminal in cells, which selects.
use std::cell::Cell;
use std::rc::Rc;

use client_core::{OpenTerminal, Pointer, SelectBy};
use gpui_kit::{DispatchPhase, Hitbox, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Window};

use super::metrics::Metrics;

/// Where a press on the grid, and the drag that follows it, are told.
#[derive(Clone)]
pub(crate) struct Pointing {
    pub(crate) open: OpenTerminal,
    /// Whether the button went down on the grid and hasn't come up, which a drag past its edges goes on from.
    pub(crate) dragging: Rc<Cell<bool>>,
}

/// Gives client-core's copy of the terminal a left press on the grid, where the pointer goes with the button down after
/// it, even past the grid's edges, and where the button comes up, each in cells from the grid's top left.
pub(super) fn follow_the_pointer(
    pointing: Pointing,
    hitbox: Hitbox,
    origin: gpui_kit::Point<Pixels>,
    metrics: &Metrics,
    window: &mut Window,
) {
    let (cell_width, line_height) = (metrics.cell_width, metrics.line_height);
    let at = move |position: gpui_kit::Point<Pixels>| Pointer {
        col: (position.x - origin.x) / cell_width,
        row: (position.y - origin.y) / line_height,
    };
    let (down, moved, up) = (pointing.clone(), pointing.clone(), pointing);
    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, _| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && hitbox.is_hovered(window) {
            down.dragging.set(true);
            down.open.press(at(event.position), select_by(event.click_count));
        }
    });
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, _| {
        if phase == DispatchPhase::Bubble && moved.dragging.get() {
            if event.pressed_button == Some(MouseButton::Left) {
                moved.open.drag(at(event.position));
            } else {
                // The button came up where the window didn't hear it, such as over another app.
                moved.dragging.set(false);
                moved.open.release();
            }
        }
    });
    window.on_mouse_event(move |event: &MouseUpEvent, phase, _, _| {
        if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && up.dragging.replace(false) {
            up.open.release();
        }
    });
}

/// What the `clicks`th press of a run selects by, as the platform counts them. Past the third, it's still the line.
fn select_by(clicks: usize) -> SelectBy {
    match clicks {
        0 | 1 => SelectBy::Cells,
        2 => SelectBy::Words,
        _ => SelectBy::Lines,
    }
}

#[cfg(test)]
mod tests {
    use client_core::SelectBy;

    use super::select_by;

    /// A double click selects a word, and a triple click the line, as the platform counts them, which is 0 for a press
    /// with no count.
    #[test]
    fn each_click_of_a_run_selects_by_more() {
        assert_eq!(
            [0, 1, 2, 3, 4].map(select_by),
            [SelectBy::Cells, SelectBy::Cells, SelectBy::Words, SelectBy::Lines, SelectBy::Lines]
        );
    }
}
