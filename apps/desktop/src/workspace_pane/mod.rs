//! One workspace: its chats, the composer, the file tab, its terminals, and the sidebar of changes beside them when
//! it's open.

pub(crate) use chat_keys::CHAT_KEY_CONTEXT;
pub use conversing::{CONVERSATION_BAR, CONVERSATION_MODE, CONVERSATION_STOP, CONVERSATION_TYPE, TAKE_OVER};
pub use dictation_ui::{LIVE_LINE, NOTHING_HEARD, SPACE_DICTATES_AFTER, SPACE_LISTENS_AFTER, SPACE_TALKS_AFTER};
pub use file_tab::Opened;
pub(crate) use keeping::drafted;
pub use microphone::{HIDE, MICROPHONE, TURN_ON};
pub use pushing::{INSTALL_APP, OPEN_GIT};
pub(crate) use reading::OnScreen;
pub use setting_up::{
    CONVERSATION_CARD, CONVERSATION_NOT_NOW, CONVERSATION_RETRY, CONVERSATION_TURN_ON, DICTATION_PROGRESS,
    SPEECH_PROGRESS,
};

mod account_wait;
mod agent;
mod attaching;
mod band;
mod changes_sidebar;
mod chat_keys;
mod chats;
mod closed;
mod composer;
mod context;
mod conversing;
mod dictating;
mod file_tab;
mod focus;
mod front;
mod keeping;
mod microphone;
mod proposal_tab;
mod pull_request;
mod pushing;
mod queue;
mod reading;
mod reserving;
mod setting_up;
mod setup;
mod subagent_tab;
mod tab;
mod tab_keys;
mod terminals;
#[cfg(test)]
mod tests;
mod view;
mod waiting;
mod workspace_card;

mod pane;
use pane::{Chat, Next};
pub use pane::{Diff, PaneEvent, Showing, WorkspacePane};
