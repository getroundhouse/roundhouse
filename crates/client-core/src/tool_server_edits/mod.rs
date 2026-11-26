//! What's typed of a tool server, to add one or to edit one the hub has, comes to before it's sent, the same for every
//! client: what an edit changes, what saving it takes for the server to know the person, what the hub is sent, and what's
//! said of that, such as what running agents will have of it. Nothing of it is sent until the person sends it, so it's
//! worked out here rather than recorded by the hub.

#[cfg(test)]
mod tests;

mod edit;
mod typed;
mod words;

pub use edit::{Edit, Sent};
pub use typed::{Does, Typed, TypedAuth, Way};
pub use words::{SIGN_IN_TO_SAVE, edit_says, key_says, way_says};
