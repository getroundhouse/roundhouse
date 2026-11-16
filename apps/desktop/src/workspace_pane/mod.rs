//! One workspace: its chats, the composer, the file tab, and the sidebar of changes beside them when it's open.

pub use dictation_ui::{LIVE_LINE, NOTHING_HEARD, SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};
pub use file_tab::Opened;
pub(crate) use keeping::drafted;
pub use microphone::{HIDE, MICROPHONE, TURN_ON};
pub use reading::Mark;
pub(crate) use reading::OnScreen;

mod agent;
mod attaching;
mod changes_sidebar;
mod chats;
mod closed;
mod composer;
mod context;
mod dictating;
mod file_tab;
mod focus;
mod front;
mod keeping;
mod microphone;
mod pull_request;
mod queue;
mod reading;
mod setup;
mod subagent_tab;
#[cfg(test)]
mod tests;
mod view;
mod waiting;
mod workspace_card;

mod pane;
use pane::Chat;
pub use pane::{Diff, PaneEvent, Showing, WorkspacePane};
