use gpui_kit::Hsla;

use crate::colors::Colors;
use crate::named::named;

named! {
    /// Where a card sits, which its surface, its edge, its shadow and its heading's colour follow. [`crate::card`]
    /// makes one of a kind.
    CardKind {
        /// Opens over what's there, such as a hover card, the workspace's card or a turn's: raised, with the one edge
        /// and the shadow.
        Over,
        /// Sits where it is, outside the chat, such as the update card or a panel in Settings: raised, with the one
        /// edge and no shadow.
        InPlace,
        /// Set in the chat's column, such as the compaction's summary or a plan once it's answered: filled as a call's
        /// block is, with no edge.
        Chat,
        /// In the chat, waiting on you, such as a plan to approve: a wash of amber, with its heading in amber.
        NeedsYou,
        /// In the chat, where the agent stopped short, such as a failed request: a wash of red, with its heading in
        /// red.
        Stopped,
    }
}

/// How much of its colour a card in the chat that's in one is washed with, over the pane.
const WASH: f32 = 0.09;

impl CardKind {
    /// What it's filled with.
    pub(super) fn surface(self, colors: &Colors) -> Hsla {
        match self {
            CardKind::Over | CardKind::InPlace => colors.raised,
            CardKind::Chat => colors.inset,
            CardKind::NeedsYou => colors.needs_you.opacity(WASH),
            CardKind::Stopped => colors.stopped.opacity(WASH),
        }
    }

    /// Whether it has the one edge around it, as what's raised does.
    pub(super) fn edged(self) -> bool {
        matches!(self, CardKind::Over | CardKind::InPlace)
    }

    /// Whether it casts the shadow, as what opens over the rest does.
    pub(super) fn casts_a_shadow(self) -> bool {
        self == CardKind::Over
    }

    /// The colour its heading's mark and words are in, when it's in one.
    pub(super) fn tint(self, colors: &Colors) -> Option<Hsla> {
        match self {
            CardKind::NeedsYou => Some(colors.needs_you),
            CardKind::Stopped => Some(colors.stopped),
            CardKind::Over | CardKind::InPlace | CardKind::Chat => None,
        }
    }

    /// How the spec's table writes it: its name.
    #[cfg(any(test, feature = "testing"))]
    pub fn written(self) -> [String; 1] {
        [format!("{self:?}")]
    }
}
