//! Chips: a thing the app knows, such as a file or a subagent, drawn with its kind's mark, its name and what's said of
//! it, in one frame whatever its kind, so a line full of different chips reads as one system. `docs/spec/components.md`
//! has the table of its kinds, which a test holds to these.

#[cfg(test)]
mod tests;

mod chip;
mod kinds;
mod sizes;
mod states;
#[cfg(test)]
pub(crate) use chip::INSIDE;
pub use chip::{Chip, chip};
pub use kinds::ChipKind;
pub use sizes::ChipSize;
pub use states::{GithubState, IssueState, PullRequestState};
