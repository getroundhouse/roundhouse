//! The issues and pull requests a reply links to on GitHub, drawn as chips among its words, whatever the link's words
//! are: each with its state's mark and its number, and its title the first time the reply links to it, and a card
//! while the pointer rests on it. A number alone, such as `#1021`, isn't one, since what it's the number of can't be
//! told. Roundhouse tells the agent to link to each. The hub looks each up on GitHub and keeps what it found for every
//! client, and the transcript asks it for those it draws, again once what it has is a minute old, so one on screen
//! says it's merged soon after it merges.
//!
//! What's selected, copied and found of one is the link's words, as the agent wrote them, since the title is the
//! app's. One the hub has nothing for, as while it's looked up, or where GitHub has no such issue the hub can see, is
//! the link as it was written. Either way a click opens the link.

mod addresses;
mod card;
mod drawing;
mod kept;
mod reading;

pub(crate) use addresses::may_link;
pub(crate) use drawing::drawing;
pub(crate) use kept::{ASK_AGAIN_AFTER, References};
pub(crate) use reading::{named, reading};
