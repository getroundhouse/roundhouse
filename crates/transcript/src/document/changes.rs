//! What changed between two versions of a Markdown text, block by block: each block at the top of either, in order,
//! and whether it's in both, or only in the one before or the one after. A block that changed is one removed, as it
//! was, followed by one added, as it is, since a block is the least a text is drawn in.

use similar::{Algorithm, DiffOp, capture_diff_slices};

use crate::texts::{TopBlock, top_blocks_and_items};

/// How a block of a text changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Change {
    /// It's in both.
    Kept,
    /// It's only in the text as it was.
    Removed,
    /// It's only in the text as it is.
    Added,
}

/// The blocks of `was` and `is`, in the order they're read: those both have, as `is` has them, and each run of those
/// only one has where it has them, `was`'s first. Blocks are the same when they say the same, but for space at their
/// ends.
pub(super) fn changed(was: &str, is: &str) -> Vec<(TopBlock, Change)> {
    let (was, is) = (top_blocks_and_items(was), top_blocks_and_items(is));
    let ops = capture_diff_slices(Algorithm::Myers, &says(&was), &says(&is));
    let taking = |blocks: Vec<TopBlock>| blocks.into_iter().map(Some).collect::<Vec<_>>();
    let (mut was, mut is) = (taking(was), taking(is));
    let mut blocks = Vec::new();
    for op in ops {
        let (old, new) = (op.old_range(), op.new_range());
        match op {
            DiffOp::Equal { .. } => blocks.extend(taken(&mut is, new, Change::Kept)),
            DiffOp::Delete { .. } | DiffOp::Insert { .. } | DiffOp::Replace { .. } => {
                blocks.extend(taken(&mut was, old, Change::Removed));
                blocks.extend(taken(&mut is, new, Change::Added));
            }
        }
    }
    blocks
}

/// What each block says, which blocks are compared by.
fn says(blocks: &[TopBlock]) -> Vec<&str> {
    blocks.iter().map(|block| block.source[..block.shown].trim()).collect()
}

/// The blocks at `range`, taken out of `blocks`, each changed as `change` says.
fn taken(
    blocks: &mut [Option<TopBlock>],
    range: std::ops::Range<usize>,
    change: Change,
) -> impl Iterator<Item = (TopBlock, Change)> + '_ {
    blocks[range].iter_mut().filter_map(Option::take).map(move |block| (block, change))
}
