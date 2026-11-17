//! How tests wait: until what they wait for holds, looking again every few milliseconds and failing once a time limit
//! has passed, or, for a test that something doesn't happen, for a bound in which it could. [`thread`]'s waits block
//! the thread they're called on, by the system's clock, and [`task`]'s wait in a tokio task, by tokio's clock.
//!
//! It's the one place a test sleeps. `clippy.toml` refuses `std::thread::sleep`, `tokio::time::sleep` and
//! `tokio::time::sleep_until` everywhere else, so no test can wait a while in place of waiting for what it means to,
//! which a slow machine loses. Elsewhere, a sleep of the product's, or of a stand-in's, `#[expect]`s the lint, saying
//! what it's for, and so does a test that sleeps on tokio's paused clock, which moves the clock rather than waiting.
//! It's a crate of its own, because the tests of every crate wait, and none of the crates they all depend on is one
//! about waiting.

#![warn(clippy::wildcard_enum_match_arm)]

mod pace;
pub mod task;
pub mod thread;
