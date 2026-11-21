//! Reading out: the logic alone, the thread that runs it against a hub, and what the bar shows.

mod bar;
mod reading;
#[cfg(unix)]
mod reading_out;
