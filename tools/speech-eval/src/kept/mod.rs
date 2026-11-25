//! The dictations someone chose to keep, heard again. They hold what a person really said, so nothing from them goes in
//! the repository: a run's folder is outside it, and a PR gives totals only.

mod became;
mod comparing;
mod hearing;
mod recordings;
mod sending;
mod words;

pub(crate) use comparing::compare;
pub(crate) use hearing::{Settings, hear};
pub(crate) use sending::sent;

#[cfg(test)]
mod tests;
