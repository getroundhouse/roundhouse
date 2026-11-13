//! The CLI's word on its subagents. Each runs as one of its tasks: `task_started` says one started, with what it was
//! asked, `task_progress` how many tokens its conversation holds as it works, and `task_notification` how it ended,
//! with its report and the count as it ended.

#[cfg(test)]
mod tests;

mod tracking;
pub(super) use tracking::Subagents;
