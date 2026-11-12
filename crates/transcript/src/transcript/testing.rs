//! What only tests read of a transcript: how much of it is loaded, and what its texts show.

use super::{Transcript, rows_and_loaded};

impl Transcript {
    /// How many rows are loaded as the session's log has them now. The list is told of them when the log says it has
    /// changed, which is after the last of the pages asked for together is in, so this can be ahead of the list.
    pub fn loaded_rows(&self) -> usize {
        self.session.read(|log| rows_and_loaded(log).1.iter().map(ExactSizeIterator::len).sum())
    }

    /// Whether every long text the rows have drawn has been read. One is read on another thread from the first time
    /// its row is drawn, and its row is measured again as it's read, so until then the session's height isn't
    /// settled.
    pub fn long_texts_read(&self, cx: &gpui_kit::App) -> bool {
        self.texts.all_read(cx) && self.prose.all_parsed()
    }

    /// What each long text the rows have drawn, and each text the agent wrote as the transcript watched, shows as
    /// of what's been read of it, and what each of the agent's texts that it draws from the kit's blocks shows.
    pub fn kept_texts(&self, cx: &gpui_kit::App) -> Vec<String> {
        let mut texts = self.texts.read(cx);
        texts.extend(self.prose.sources());
        texts
    }
}
