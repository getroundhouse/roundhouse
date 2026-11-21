//! One workspace's chat: the transcript the desktop app shows, and under it a composer, or what the agent asks in its
//! place, and over it, what a line in it opens.

pub(crate) use view::{ChatEvent, ChatView};

mod chats;
mod draft;
mod over;
mod reading;
mod settings;
mod view;
