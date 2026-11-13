//! What sits beside Send: a ring for how full the chat's context is, and a bar for each of the account's limits.
//! Each shows a card while the pointer rests on it: what the context holds, or how much of the limits is left and
//! whose account it is.

mod card;
mod words;

mod usage;

pub(super) use usage::Usage;
