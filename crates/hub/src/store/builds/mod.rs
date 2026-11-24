//! Which build each chat's agent last ran on, so a chat can say when an update moved its agent onto another.

#[cfg(test)]
mod tests;

mod build_rows;
#[cfg(test)]
pub(in crate::store) use build_rows::NEWEST_START;
