//! An area in a row that scrolls what it holds under the wheel until it reaches the end the wheel is turning toward,
//! and then leaves the wheel to the transcript, so a long block can be read into the rest of the chat in one motion.
//! It only takes a gesture that starts over it, as [`wheel`] says, so one that starts on the transcript carries on
//! over it, and a trackpad's fling stops at its end instead of carrying the transcript on at its speed.
//!
//! GPUI's own `overflow_y_scroll` never gets the wheel here, and the kit's areas keep it even at their ends. The
//! transcript's list takes each wheel event in the capture phase, after everything inside it has had the chance and
//! before an area's own listener would see it. So this area acts in the capture phase too, after what it holds, and
//! stops the event only when it moved, or when it keeps its own fling from the transcript.
//!
//! An area of a file's lines scrolls sideways too, so a long line can be read to its end. It does that itself, as it
//! scrolls down, so a gesture moves only what it went to, and GPUI never moves it sideways. [`holding_wide`] lays it
//! out, and an edit's preview too, which keeps the wheel for itself and leaves GPUI to scroll it both ways.

mod wheel;

pub(crate) use wheel::hears_the_wheel;

mod scrolling;
pub(crate) use scrolling::{ScrollArea, holding_wide};
