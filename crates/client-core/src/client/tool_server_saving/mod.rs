//! Adding a tool server, or saving an edit of one, as one call, which is let go of as what started it lets go of it,
//! while the hub asks the server for its tools: with the hub's save, or, with a hub from before it, with a check and
//! then the add or the edit.

#[cfg(test)]
mod tests;

mod saving;
pub use saving::ToolServerSaving;
