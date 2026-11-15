//! Background PR reads: choose what's due, and start a job to read it, which spends its calls from the shared budget
//! once it has a worker to read through, and lets go of that worker once it's done. A read in turn is only started
//! while the budget can spare its calls.

mod scheduler;
#[cfg(test)]
mod tests;
