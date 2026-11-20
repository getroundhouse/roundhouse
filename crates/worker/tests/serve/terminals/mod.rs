//! A worker's terminals, driven as a hub drives them, each running `sh` in a PTY.

mod attached;
mod hub;
mod running;
mod slow;
