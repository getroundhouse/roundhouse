//! Every size a bar and a row of a list come in. A bar's size is chosen by where it sits, and every bar along the top
//! of a pane is as tall, so the bars side by side across the window line up. A row's is chosen by what its list is, so
//! the rows of the same kind of list are as tall, wherever it is. `docs/spec/look.md` has a table of each, which tests
//! hold to these.

#[cfg(test)]
mod tests;

mod bars;
mod rows;
pub use bars::BarSize;
pub use rows::RowSize;
