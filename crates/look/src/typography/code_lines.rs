use gpui_kit::{Pixels, px};

use crate::named::named;
use crate::traits::Reach;

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
    /// How far apart its lines are, from one's top to the next's, at the reach this process draws for.
    pub fn height(self) -> Pixels {
        self.height_at(Reach::now())
    }

    /// How far apart its lines are at `reach`. On a handheld, code among the chat's prose is on the lines iOS sets its
    /// own text on at code's size there, and code read line by line is the desk's 3px further apart than that.
    pub(crate) fn height_at(self, reach: Reach) -> Pixels {
        match (reach, self) {
            (Reach::Desk, CodeLines::File) => px(20.),
            (Reach::Desk, CodeLines::Chat) => px(17.),
            (Reach::Handheld, CodeLines::File) => px(22.),
            (Reach::Handheld, CodeLines::Chat) => px(19.),
        }
    }

    /// How the spec's table writes it: its name, and its height at a desk and on a handheld.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 3] {
        let height = |reach| format!("{}px", f32::from(self.height_at(reach)));
        [format!("{self:?}"), height(Reach::Desk), height(Reach::Handheld)]
    }
}
