//! Each project's link to its repository on GitHub, by GitHub's ID for it: finding which repository a project's address
//! on github.com names, as every connected account sees it, and reading what GitHub says of each linked repository by
//! its ID, which is kept as an observation, so a rename or a move to another owner changes what's read and leaves the
//! link. And what the rest of the hub reads of a project's link: which repository on GitHub it is now, and what a
//! machine with no checkout clones. And adding a project by its link alone, from the repositories the hub's accounts can
//! see, or an address.

mod adding;
mod linking;
mod listing;
pub(crate) use listing::SeenLately;
#[cfg(test)]
mod tests;
mod using;
