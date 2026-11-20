//! `ssh` on the hub's own machine: whose key GitHub takes from it, as GitHub names it to `ssh -T git@github.com`, so
//! Settings' Providers can say who agents on the local provider push as over SSH. Nothing else in the hub runs `ssh`, or
//! knows what it prints.

mod reading;
/// Against a stand-in for `ssh` that's a shell script.
#[cfg(test)]
#[cfg(unix)]
mod tests;

pub(crate) use reading::Ssh;
