//! Restarting the hub, across a real break or to finish an update, and the app, to install one. Each can lose
//! something, agents that stop or what's typed and not sent, so each says what first. Restarting the hub now asks in a
//! dialog, since it stops agents, and restarting the app in the card at the bottom of the sidebar.

mod asking;

mod older_hub;
#[cfg(any(test, feature = "testing"))]
pub(super) use older_hub::LOOK_AGAIN;
pub(super) use older_hub::OlderHub;
pub use older_hub::UpdateQuestion;
