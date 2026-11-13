//! How long things take to move, so everything that moves the same way takes the same time, and the ways a value
//! moves. One that a single view draws follows the kit's `transition`, with a policy from here. One that several
//! views lay out by is a [`Sliding`], kept where they can all read it. A scroll to a place that can move meanwhile
//! is a [`Scrolling`], which says what share of the way is left rather than where to be, since a slide between two
//! values set as it starts can't follow a place that moves. Each skips the motion under Reduce Motion, and asks for
//! frames only while it moves. What scrolls on under a pointer held past its edge is [`ScrollingOn`]. `docs/gpui.md`
//! has the rules.

mod edge;

pub use edge::ScrollingOn;

mod transitions;
pub use transitions::{FADE, PANEL, SCROLL, Scrolling, Sliding, fade};
