//! `gh`, GitHub's command line tool, on the hub's own machine: who it's logged in to github.com as, and the token it
//! uses, which the hub reads to use that login as a GitHub account's, as
//! `docs/decisions/github-on-provisioned-machines.md` says. Nothing else in the hub knows `gh`'s flags or what it
//! prints.

mod reading;
mod running;
/// Against a stand-in for `gh` that's a shell script.
#[cfg(test)]
#[cfg(unix)]
mod tests;
mod token;

pub(crate) use reading::{Gh, GhRead, GhTokens};
