//! Putting a build where a hub installed as a service runs it from, as the hub's user, so the files are theirs and only
//! theirs, as the rest of the hub's data directory is.

mod placed;
#[cfg(test)]
mod tests;

pub(crate) use placed::place;
