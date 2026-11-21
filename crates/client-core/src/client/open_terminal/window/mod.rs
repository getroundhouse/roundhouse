//! A client's window onto a terminal, kept from the pictures and the rows the hub sends of it: the rows near its view,
//! where the view is scrolled to and what's selected, by row number, and what it asks the hub for. It holds nothing
//! of Ghostty's, which only the worker has.

mod fitting;
mod painting;
mod pointing;
mod rows;
mod selecting;
#[cfg(test)]
mod tests;
mod viewing;

pub(super) use selecting::Step;
pub use selecting::{Pointer, SelectBy};
pub(super) use viewing::{Ask, Replied, Window};
