#![cfg(test)]
mod approvals;
mod async_agents;
mod async_wakeup;
mod background;
mod calls;
mod compaction;
mod decisions;
mod harness;
mod lifecycle;
mod models;
mod prompt_intake;
#[cfg(unix)]
mod real_cli;
mod transitions;
mod wakeup;
