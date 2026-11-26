//! Adding a tool server, or saving an edit of one, as one call, which is let go of as what started it lets go of it,
//! while the hub asks the server for its tools.

#[cfg(test)]
mod tests;

mod saving;
pub use saving::ToolServerSaving;
