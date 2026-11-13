//! One workspace: its chats, the composer, the file tab, and the sidebar of changes beside them when it's open.

pub use file_tab::Opened;
pub(crate) use keeping::drafted;
pub use reading::Mark;

mod agent;
mod attaching;
mod changes_sidebar;
mod chats;
mod closed;
mod composer;
mod context;
mod file_tab;
mod focus;
mod front;
mod keeping;
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
pub(crate) use pane::HEADER_HEIGHT;
pub use pane::{Diff, PaneEvent, Showing, WorkspacePane};
