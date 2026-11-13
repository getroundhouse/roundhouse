mod environment;
#[cfg(test)]
mod tests;

#[cfg(test)]
use environment::{InChildren, inherited, leave, outside};
pub(super) use environment::{RUNNING_FROM, leave_behind, sharing};
#[cfg(test)]
use std::{os::fd::RawFd, path::Path};
