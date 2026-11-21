//! Moving the end of what's selected to the line above or below, by where the texts drawn in the last frame were laid
//! out. As a drag goes by what shows, so does a move, but for the end's own text, any line of which is one to go to,
//! whether an area that scrolls it, or the list, shows it or not, since the area scrolls to it, and the list does. What
//! an area hides of another text is laid out where other texts are drawn, so it isn't one to go to. It goes to the
//! place nearest across to where it was, which a run of them keeps to, so a run past a shorter line comes back to it
//! after. A line of a table's cell is above or below only the cells under or over it, not those beside it.
//!
//! GPUI draws the place where a row wraps at the end of the row before, so a run keeps which of the two rows the end
//! is on, and goes on from the one it moved to.

mod nearest;
#[cfg(test)]
mod tests;

pub(super) use nearest::Run;
#[cfg(test)]
use nearest::{Line, move_a_line};
