//! The optional microphone in the composer bar, sharing its presentation with the new-workspace prompt.

use super::{WorkspacePane, composer::Slot};
use gpui_kit::Context;

pub use crate::dictating::{HIDE, MICROPHONE, TURN_ON};

impl WorkspacePane {
    pub(super) fn microphone(&self, cx: &mut Context<Self>) -> Option<Slot> {
        crate::dictating::microphone(&self.dictation, cx).map(Slot::holding)
    }
}
