//! What Claude's adapter recorded before it typed its calls, typed as it records them now: each call by the tool's name
//! and input, what came back for each, the call each request for approval asked to make, and what a subagent's record
//! said it had done lately. No other adapter recorded those forms. The store's migration types what the hub kept, and
//! the worker link what a worker of an older build still sends, so nothing past them reads the older forms.
//!
//! It's frozen, as Claude's tools and words were when the adapter stopped recording them, and it never changes, while
//! the adapter's own table follows the CLI, so they aren't one. A result is read only from what the adapter kept of it,
//! since what the CLI said beside it is only in the event's raw line, which nothing reads again.

mod asking;
mod calls;
mod lately;
mod older;
mod outputs;
#[cfg(test)]
mod tests;
mod typing;

pub(crate) use lately::lately;
pub(crate) use typing::Typing;
