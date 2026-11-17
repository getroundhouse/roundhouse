//! The address a checkout fetches from, which a hub keeps on its repository's record, and whether a push from it goes
//! through the worker's git credential.

mod reading;
mod route;
#[cfg(test)]
mod tests;

pub use reading::repo_url;
