//! Which blocks are open. A block the person opened or closed stays as they left it, and the rest start the way
//! their kind does. Something in a block can open on its own, such as a command shown whole in its opened call, so a
//! choice is kept for each kind of fold in an item.

use std::collections::HashMap;

use protocol::event::ContentBlock;

use crate::item_key::ItemKey;

/// A kind of block that opens and closes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Fold {
    Thinking,
    /// What a tool sent back.
    Output,
    /// An opened call's command, shown whole rather than cut off at the end of its line.
    Command,
}

impl Fold {
    /// The kind of fold a block is. Only thinking and a tool's output open, and anything else is drawn whole, so it
    /// never asks. A command shown whole is a fold inside its call's block, rather than a kind of block.
    pub(crate) fn of(block: &ContentBlock) -> Self {
        if matches!(block, ContentBlock::Thinking { .. }) { Fold::Thinking } else { Fold::Output }
    }
}

#[derive(Default)]
pub(crate) struct Folds {
    /// What the person chose, for each fold they've opened or closed, by its kind, so finding one borrows its key.
    chosen: HashMap<Fold, HashMap<ItemKey, bool>>,
    /// Whether thinking starts open. Output always starts closed, since it's mostly long and rarely read.
    thinking_open: bool,
}

impl Folds {
    pub(crate) fn is_open(&self, key: &ItemKey, fold: Fold) -> bool {
        self.chosen.get(&fold).and_then(|chosen| chosen.get(key)).copied().unwrap_or(match fold {
            Fold::Thinking => self.thinking_open,
            Fold::Output | Fold::Command => false,
        })
    }

    pub(crate) fn choose(&mut self, key: ItemKey, fold: Fold, open: bool) {
        self.chosen.entry(fold).or_default().insert(key, open);
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
        folds.choose(block(0), Fold::Thinking, true);
        folds.set_thinking_open(true);
        folds.choose(block(1), Fold::Thinking, false);

        folds.set_thinking_open(false);
        assert!(folds.is_open(&block(0), Fold::Thinking));
        folds.set_thinking_open(true);
        assert!(!folds.is_open(&block(1), Fold::Thinking));
        assert!(folds.is_open(&block(2), Fold::Thinking), "one nobody chose follows the setting");
    }

    #[test]
    fn a_command_shown_whole_is_its_own_choice_from_its_call_being_open() {
        let mut folds = Folds::default();
        folds.choose(block(0), Fold::Output, true);
        assert!(!folds.is_open(&block(0), Fold::Command), "a command starts cut off");

        folds.choose(block(0), Fold::Command, true);
        folds.choose(block(0), Fold::Output, false);

        assert!(folds.is_open(&block(0), Fold::Command), "and stays whole while its call is closed");
        assert!(!folds.is_open(&block(0), Fold::Output));
    }
}
