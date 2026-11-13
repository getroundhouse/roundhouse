//! Titles, and what each way of setting one may replace, so that rule is written once for workspaces and chats.

#[cfg(test)]
use super::{Store, StoreError, lock};
#[cfg(test)]
use protocol::model::{Title, TitleSource};

#[cfg(test)]
mod tests;

mod title_rows;
pub use title_rows::{Retitle, Retitled};
