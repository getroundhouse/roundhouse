use gpui_kit::{Pixels, px};

use crate::named::named;

named! {
    /// How far apart lines of code are, by where they're read. Code is one size everywhere, and its lines are spaced
    /// by whether they're read one by one or glanced at among prose. The terminal lays out a grid of its own.
    CodeLines {
        /// Read line by line, often beside their numbers: a file, a diff, and a call's numbered lines.
        File,
        /// Glanced at among the chat's prose: a code block in a reply, and what a call was given and put out, however
        /// long it is.
        Chat,
    }
}

impl CodeLines {
    /// How far apart its lines are, from one's top to the next's.
    pub const fn height(self) -> Pixels {
        match self {
            CodeLines::File => px(20.),
            CodeLines::Chat => px(17.),
        }
    }

    /// How the spec's table writes it: its name and its height.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 2] {
        [format!("{self:?}"), format!("{}px", f32::from(self.height()))]
    }
}
