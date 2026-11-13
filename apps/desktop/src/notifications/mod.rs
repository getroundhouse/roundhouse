//! Which chats have come to need the person, for a notification and a sound: one whose agent finished with a reply
//! nobody has read, stopped short, or is waiting on them for an answer or an approval. Never the chat being read, the
//! one showing in the window in front, since it's already in view.

#[cfg(test)]
mod tests;

mod watch;
pub(crate) use watch::{APP_ID, Came, Watch};
#[cfg(test)]
pub(crate) use watch::{Changes, Why};
