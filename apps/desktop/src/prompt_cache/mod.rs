//! What the app says of a chat's prompt cache outside its transcript, and when that changes: a clock on its tab and
//! its workspace's row once it's about to expire, how long it has left on Home, a line above the composer once it has
//! expired, and a note on the other models in the picker while it's warm. Each says it only in a chat big enough for
//! writing the cache again to cost something, as [`Session::cache_worth_telling`](protocol::model::Session::cache_worth_telling) says. `docs/spec/chats.md` has the
//! rules.

#[cfg(test)]
use protocol::model::Session;

#[cfg(test)]
mod tests;

mod expiration;
#[cfg(test)]
use expiration::LONGEST_WAIT_MS;
pub(crate) use expiration::{
    clock, composer_line_changes_in_ms, expired_line, expiring_in_ms, next_change_in_ms, soonest_expiring,
    switching_note, told_of,
};
