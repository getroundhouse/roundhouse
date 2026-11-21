//! A window onto a terminal, fed the pictures a worker's copy of it sends, against what Ghostty showed of the same
//! output. What Ghostty showed was recorded from the copy of Ghostty the client kept before it showed pictures, which
//! the window was checked equal to then.

mod described;
mod feeding;
mod fitting;
mod painting;
mod scrolling;
mod selecting;
