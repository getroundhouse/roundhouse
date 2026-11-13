//! Keeping what the API's cache holds of the conversation, from what the agent's adapter says of each of its requests.

#[cfg(test)]
use protocol::event::EventBody;
#[cfg(test)]
use protocol::model::{CacheRequest, PromptCache};

#[cfg(test)]
mod tests;

mod expiration;

#[cfg(test)]
use expiration::{after, found_expired};
