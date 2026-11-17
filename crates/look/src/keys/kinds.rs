use crate::named::named;

named! {
    /// How a key is shown, by where it is. [`key`] makes one of a kind.
    KeyKind {
        /// On a small raised key of its own, with what's held smaller than the key pressed on a Mac, as on its keys,
        /// set on its own beside what it does, such as `⌘ F` at the end of Settings' search.
        Capped,
        /// Written as text, in the colour of what it's in, such as the Return after the words of the button it presses.
        Plain,
    }
}

impl KeyKind {
    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
