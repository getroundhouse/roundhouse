//! Following through `Subscribe`: what each level is sent first, which is where it stands, and then every change.

mod hub_level;
mod notices;
mod session_level;
mod starting;
#[cfg(test)]
mod tests;
mod workspace_level;

pub(super) use starting::subscribe;
