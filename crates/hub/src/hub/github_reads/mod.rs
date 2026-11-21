//! Every read of GitHub's issues and pull requests that goes through the hub's cache, [`crate::github_cache`]: what's
//! asked for is answered from the cache where it's as fresh as the ask needs, and read, once however many ask,
//! where it isn't. *Not built:* only chats' chips read through it.

mod reading;
mod references;
#[cfg(test)]
mod tests;
