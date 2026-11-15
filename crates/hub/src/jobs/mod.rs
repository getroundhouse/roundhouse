//! The hub's jobs: the work it does on its own, which no request waits on, such as reading pull requests nobody asked
//! for. Each runs on a task of its own, so whatever decides what's due only starts jobs and never waits on one, and a
//! job that hangs holds up only what can't run beside it, until its limit. One still running at its limit is stopped
//! where it is, which releases what it held, and the log says which job it was, what it was about and where it had got
//! to. Then its after-step runs, however it ended, within a limit of its own, to put back what the job may have left,
//! such as a worker it started. A job's work is written to be dropped at any `await`, since that's how it's stopped,
//! but for what it runs [`uncut`], which mustn't stop partway once it has begun.

mod kind;
mod progress;
mod running;
#[cfg(test)]
mod tests;
mod uncut;

pub(crate) use kind::JobKind;
use progress::Progress;
pub(crate) use progress::step;
pub(crate) use running::{Ended, Jobs, Limits};
pub(crate) use uncut::uncut;
