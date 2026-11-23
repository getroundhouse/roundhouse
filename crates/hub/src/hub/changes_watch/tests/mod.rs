//! The hub telling a workspace's worker whether to watch its worktree: as subscriptions follow it and stop, as a move
//! makes it ready or not, and as its worker goes or doesn't answer.

mod following;
mod moving;
mod stand_in;
mod workers;
