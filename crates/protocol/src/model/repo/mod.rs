//! A repository's record: where the repository is, by its checkout on the hub's machine, its address, or both, and its
//! link to the repository on its host.

mod github;
mod link;
mod origin;
mod record;
#[cfg(test)]
mod tests;
mod url;

pub use link::{AddressForm, Linking, RepoLink, Unlinked};
pub use origin::RepoOrigin;
pub use record::Repo;
pub use url::{RepoUrl, RepoUrlError};
