//! One workspace: its chats, the composer, the file tab, and the sidebar of changes beside them when it's open.

pub use dictating::{LIVE_LINE, NOTHING_HEARD};
pub use file_tab::Opened;
pub use holding_space::{SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER};
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
mod holding_space;
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
