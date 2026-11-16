//! The device flow, in two halves, so no test waits out an interval or a time limit. What GitHub is asked, and what
//! each of its answers comes to, is against a stand-in for GitHub on 127.0.0.1, one call at a time, on tokio's clock
//! as it runs. How often GitHub is asked, and for how long, is on tokio's paused clock, with each answer as the test
//! says. They're apart because a paused clock moves on whenever every task waits, which a request to the stand-in
//! does while its answer is on its way, so the clock can reach the end of [`ASK_WITHIN`] first.

use super::*;

mod answers;
mod pacing;
