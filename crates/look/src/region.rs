//! The parts of the window that tests find by where they're drawn, such as the gallery cutting a screen's picture down
//! to one. Marking a part gives it its ID and lets a test find it in one call, so neither can be left out, and a test
//! that names a part names a [`Region`], which only [`InRegion::region`] puts on an element. They're named here because
//! the app and `transcript` each mark some, and `transcript` can't depend on the app.

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
    /// The bar along the top of what the file tab shows, with the file's path.
    FileBar,
    /// The terminal in front, in the chat's place.
    Terminal,
    /// What the proposal tab shows, in the chat's place.
    Proposal,
    /// Where a prompt is written.
    Composer,
    /// The prompts waiting to be sent, above the composer.
    Queue,
    /// The setup script's log, above the composer.
    SetupLog,
    /// What a push from the workspace would meet on GitHub, above the composer, while the default can't push there.
    PushLine,
    /// That the chat's account is using what's reserved of its week, above the composer.
    ReserveLine,
    /// That the chat couldn't start, since no account had usage to spare, at the chat's end, with the accounts to start
    /// it on.
    AccountWait,
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
    /// Conversation mode's bar, in the composer's place.
    ConversationBar,
    /// What an edit changed, shown while the pointer is on its line.
    EditPreview,
    /// A finished turn's models, times and tokens, shown while the pointer is on its last line.
    TurnCard,
    /// How full the chat's context is, shown from the composer.
    ContextCard,
    /// What's left of the account's limits, shown from the composer.
    UsageCard,
    /// What the composer's microphone opens until dictation is ready: the offer to turn it on, or how it stands.
    DictationCard,
    /// What conversation mode's button opens until dictation and speech are ready: how each stands, and turning them
    /// on.
    ConversationCard,
    /// Home, which lists every workspace.
    Home,
    /// The card that says what an update does, above its card in the sidebar.
    UpdateDetails,
    /// Where a workspace's worktree is and which build its agents are on, with any of its warnings, shown from its name
    /// in the header.
    WorkspaceCard,
    /// A workspace's warnings alone, shown from the pull request's bar when they're why it can't say where the pull
    /// request stands.
    WarningsCard,
    /// What an account's row in Settings has no room for, shown from the row.
    AccountCard,
    /// What the version's place says in full, such as what went wrong updating, shown from it.
    BuildLineCard,
    /// Where a build from a checkout was made and keeps its data, shown from its card in the sidebar.
    CheckoutDetails,
    /// What an issue or a pull request a reply links to is, and where it stands, shown while the pointer is on its chip.
    ReferenceCard,
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
            Region::FileBar => "file-bar",
            Region::Terminal => "terminal",
            Region::Proposal => "proposal",
            Region::Composer => "composer",
            Region::Queue => "queue",
            Region::SetupLog => "setup-log-text",
            Region::PushLine => "push-line",
            Region::ReserveLine => "reserve-line",
            Region::AccountWait => "account-wait",
            Region::Transcript => "transcript",
            Region::Outline => "outline",
            Region::FindBar => "find-bar",
            Region::LatestButton => "latest-button",
            Region::Question => "question",
            Region::ConversationBar => "conversation-bar",
            Region::EditPreview => "edit-preview",
            Region::TurnCard => "turn-card",
            Region::ContextCard => "context-card",
            Region::UsageCard => "usage-card",
            Region::DictationCard => "dictation-card",
            Region::ConversationCard => "conversation-card",
            Region::Home => "home-page",
            Region::UpdateDetails => "update-details-card",
            Region::WorkspaceCard => "workspace-card",
            Region::WarningsCard => "warnings-card",
            Region::AccountCard => "account-card",
            Region::BuildLineCard => "build-line-card",
            Region::CheckoutDetails => "checkout-details-card",
            Region::ReferenceCard => "reference-card",
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
