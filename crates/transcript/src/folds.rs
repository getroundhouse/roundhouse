//! Which blocks are open. A block the person opened or closed stays as they left it, and the rest start the way
//! their kind does.

use std::collections::HashMap;

use protocol::event::ContentBlock;

use crate::item_key::ItemKey;

/// A kind of block that opens and closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fold {
    Thinking,
    /// What a tool sent back.
    Output,
    /// An image a prompt carried, drawn small until it's opened.
    Picture,
}

impl Fold {
    /// The kind of fold a block is. Only thinking and a tool's output open, and anything else is drawn whole, so it
    /// never asks.
    pub(crate) fn of(block: &ContentBlock) -> Self {
        if matches!(block, ContentBlock::Thinking { .. }) { Fold::Thinking } else { Fold::Output }
    }
}

#[derive(Default)]
pub(crate) struct Folds {
    /// What the person chose, for each block they've opened or closed.
    chosen: HashMap<ItemKey, bool>,
    /// Whether thinking starts open. Output always starts closed, since it's mostly long and rarely read, and so does
    /// a picture, which would push the reply off screen.
    thinking_open: bool,
    /// How many choices have been made, so what depends on them can tell when to look again.
    chosen_count: u64,
}

impl Folds {
    pub(crate) fn is_open(&self, key: &ItemKey, fold: Fold) -> bool {
        self.chosen.get(key).copied().unwrap_or(match fold {
            Fold::Thinking => self.thinking_open,
            Fold::Output | Fold::Picture => false,
        })
    }

    pub(crate) fn choose(&mut self, key: ItemKey, open: bool) {
        self.chosen.insert(key, open);
        self.chosen_count += 1;
    }

    /// How many times a block has been opened or closed by hand.
    pub(crate) fn chosen_count(&self) -> u64 {
        self.chosen_count
    }

    pub(crate) fn thinking_open(&self) -> bool {
        self.thinking_open
    }

    /// Returns whether that changed anything.
    pub(crate) fn set_thinking_open(&mut self, open: bool) -> bool {
        std::mem::replace(&mut self.thinking_open, open) != open
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(index: u32) -> ItemKey {
        ItemKey::Block("m".into(), index)
    }

    #[test]
    fn thinking_starts_as_the_setting_says_and_output_starts_closed() {
        let mut folds = Folds::default();
        assert!(!folds.is_open(&block(0), Fold::Thinking));

        assert!(folds.set_thinking_open(true));

        assert!(folds.is_open(&block(0), Fold::Thinking));
        assert!(!folds.is_open(&block(1), Fold::Output));
        assert!(!folds.set_thinking_open(true), "already open");
    }

    #[test]
    fn a_block_the_person_chose_stays_as_they_left_it_whatever_the_setting() {
        let mut folds = Folds::default();
        folds.choose(block(0), true);
        folds.set_thinking_open(true);
        folds.choose(block(1), false);

        folds.set_thinking_open(false);
        assert!(folds.is_open(&block(0), Fold::Thinking));
        folds.set_thinking_open(true);
        assert!(!folds.is_open(&block(1), Fold::Thinking));
        assert!(folds.is_open(&block(2), Fold::Thinking), "one nobody chose follows the setting");
    }
}
