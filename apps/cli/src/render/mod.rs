//! Plain text for a terminal. This is a debugging view, not the product's transcript.

mod approval;

#[cfg(test)]
mod tests;

mod printer;

pub(crate) use printer::Printer;
use printer::about;
#[cfg(test)]
use printer::{called, came_back, denial, started};
