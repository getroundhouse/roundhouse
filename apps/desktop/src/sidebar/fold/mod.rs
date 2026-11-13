//! A project's workspaces folding away under its row as it's collapsed, and back out. The workspaces stay where they
//! are under their project's row, and whatever comes after the project slides up over them or down off them, so
//! they're cut off at its top edge, and it's plain where they went. Nothing here keeps time: what comes after is sent
//! to its place as any line that moves is, by [`Places`](crate::reorder::Places), and the fold is wherever that has
//! got to, so the two can't come apart, and a fold turned back partway goes on from where it was.

#[cfg(test)]
mod tests;

mod folding;
pub(super) use folding::{Drawn, Fold, Laid, folds, lay, rests};
