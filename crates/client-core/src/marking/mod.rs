//! Marking chats read, or a reply unread again, as every client offers it: a chat at a time, or a whole workspace from
//! its row, reading the chat a client shows, and whether a chat or a workspace needs a look.

mod attention;
mod mark;
mod reading;
pub use attention::{needs_a_look, waiting_on_you};
pub use mark::{Mark, workspace_marking};
pub use reading::Reading;

#[cfg(test)]
mod tests;
