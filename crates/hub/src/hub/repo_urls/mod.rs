//! The address each repository's checkout fetches from, which a worker on the hub's machine reads for its record,
//! since the hub runs no git, as `ReadRepoUrl` says.

#[cfg(test)]
mod tests;

mod reading;
#[cfg(test)]
use reading::READ_WITHIN;
