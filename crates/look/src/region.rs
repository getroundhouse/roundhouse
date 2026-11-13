//! The parts of the window that tests find by where they're drawn, such as the gallery cutting a screen's picture down
//! to one. Marking a part gives it its ID and lets a test find it in one call, so neither can be left out, and a test
//! that names a part names a [`Region`], which only [`InRegion::region`] puts on an element.

use gpui_kit::base::ObservedElement;
use gpui_kit::{Div, Stateful, TestSupportExt as _};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// The sidebar down the window's left side.
    LeftSidebar,
    /// The sidebar of changes, down its right side.
    RightSidebar,
    /// The workspace's name and branch, along the top.
    Header,
    /// The chat's tabs, under the header.
    TabBar,
    /// What the file tab shows, in the chat's place.
    OpenFile,
    /// Where a prompt is written.
    Composer,
    /// The prompts waiting to be sent, above the composer.
    Queue,
    /// The setup script's log, above the composer.
    SetupLog,
    /// The chat's rows.
    Transcript,
    /// The ticks at the right of a chat with several prompts, one for each, at the pane's edge.
    Outline,
    /// The bar that finds text in the chat, over its top.
    FindBar,
    /// The button back down to the chat's latest row, which shows once the chat is scrolled away from it.
    LatestButton,
    /// What the agent asks in the composer's place: a question, or to use a tool.
    Question,
    /// What an edit changed, shown while the pointer is on its line.
    EditPreview,
    /// A finished turn's models, times and tokens, shown while the pointer is on its last line.
    TurnCard,
    /// How full the chat's context is, shown from the composer.
    ContextCard,
    /// What's left of the account's limits, shown from the composer.
    UsageCard,
    /// Home, which lists every workspace.
    Home,
    /// The card that says what an update does, above its card in the sidebar.
    UpdateDetails,
    /// Where a workspace's worktree is and which build its agents are on, shown from its name in the header.
    WorkspaceCard,
}

impl Region {
    /// The element's ID, which tests can also find it by.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Region::LeftSidebar => "left-sidebar",
            Region::RightSidebar => "right-sidebar",
            Region::Header => "workspace-header",
            Region::TabBar => "tab-bar",
            Region::OpenFile => "open-file",
            Region::Composer => "composer",
            Region::Queue => "queue",
            Region::SetupLog => "setup-log-text",
            Region::Transcript => "transcript",
            Region::Outline => "outline",
            Region::FindBar => "find-bar",
            Region::LatestButton => "latest-button",
            Region::Question => "question",
            Region::EditPreview => "edit-preview",
            Region::TurnCard => "turn-card",
            Region::ContextCard => "context-card",
            Region::UsageCard => "usage-card",
            Region::Home => "home-page",
            Region::UpdateDetails => "update-details-card",
            Region::WorkspaceCard => "workspace-card",
        }
    }
}

/// Marks an element as a [`Region`], in the place its ID would go. Outside tests it only gives it the ID, and it
/// returns the type `.id(…).test_support()` would, so it's built on in the same ways.
pub trait InRegion {
    #[must_use]
    fn region(self, region: Region) -> ObservedElement<Stateful<Div>>;
}

impl InRegion for Div {
    fn region(self, region: Region) -> ObservedElement<Stateful<Div>> {
        gpui_kit::InteractiveElement::id(self, region.id()).test_support()
    }
}
