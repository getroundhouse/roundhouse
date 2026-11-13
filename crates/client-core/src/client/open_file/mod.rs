//! A file as a commit held it, opened to be read a page of lines at a time as a view looks at it, which is how a diff
//! shows what git left out of a file too big to send whole. It's opened, and the pages a view wants are fetched, on a
//! thread of its own, which says when there's more to show, so a view only reads what's held.

#[cfg(test)]
mod tests;

mod loading;
pub use loading::{OpenFile, Opening};
