//! A changed file that isn't text, as the workspace's file tab shows it in place of the lines it hasn't got: an image as
//! it was and as it is, side by side, one side at a time in one place, flipped between, or as what changed between
//! them, and any other file saying why it can't be shown. The diff says which sides there are, and the hub reads each:
//! the one before from the commit the diff was taken from, and the one after from the worktree. Which file is the file
//! diff's to say.

mod difference;
mod drawing;
mod stacked;

#[cfg(test)]
mod tests;

mod comparison;
#[cfg(test)]
use comparison::side;
pub use comparison::{Comparison, ImageDiff, KEY_CONTEXT, Shown, comparison};
use comparison::{Outcome, Side, choose};
