//! Which build each workspace's worker is, while it's another than the hub's, and which build each chat's agent last
//! ran on, so a chat can say when an update moved its agent onto another.

#[cfg(test)]
mod tests;

mod build_rows;
