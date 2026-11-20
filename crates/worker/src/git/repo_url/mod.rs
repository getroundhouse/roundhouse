//! The address a checkout fetches from, which a hub keeps on its repository's record.

mod reading;
#[cfg(test)]
mod tests;

pub use reading::repo_url;
