//! A window onto a terminal, fed the pictures a worker's copy of it sends, against what the client's own copy of
//! Ghostty showed of the same output.

mod feeding;
mod fitting;
mod painting;
mod scrolling;
mod selecting;
