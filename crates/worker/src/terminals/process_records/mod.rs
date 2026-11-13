//! What a terminal records of the processes it runs, as [`process::ProcessRecord`]s beside the worker's of its agents, so the
//! provider stops them if the worker is killed, as "When a process dies" in `docs/architecture.md` says: the shell, and
//! the job it runs in front, which has a group of its own and can outlast the shell, as one that ignores being hung up
//! on does. These aren't the hub's record of a terminal, which is another thing.

mod keeping;
#[cfg(test)]
mod tests;

pub(super) use keeping::ProcessRecords;
