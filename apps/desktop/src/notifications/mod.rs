//! Which chats have come to need the person, for a notification and a sound, and how many workspaces have one, for the
//! Dock's badge: one whose agent finished with a reply nobody has read, stopped short, or is waiting on them for an
//! answer or an approval. The chat being read, the one showing in the window in front, is never told of, since it's
//! already in view, though it counts on the badge while it needs them, as its row stays bright in the sidebar.

#[cfg(test)]
mod tests;

mod watch;
pub(crate) use watch::{APP_ID, Came, Watch};
#[cfg(test)]
pub(crate) use watch::{Changes, Why};
