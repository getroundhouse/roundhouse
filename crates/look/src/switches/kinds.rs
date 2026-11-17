use crate::named::named;

named! {
    /// How a switch is drawn, by where it is. [`crate::switch`] makes one of a kind.
    SwitchKind {
        /// Its segments small, on a raised track as tall as a regular button, such as the diff's views at the right
        /// end of a file's path.
        Tracked,
        /// Its segments regular borderless buttons, with nothing around them, such as the image comparison's or where
        /// a GitHub app is made.
        Trackless,
    }
}

impl SwitchKind {
    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
