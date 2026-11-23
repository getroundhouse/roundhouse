//! The environment the hub gives everything it starts on its machine: what the user's login shell has once it has read
//! their profile, started in the hub's own environment, so the profile builds on it as it does in a terminal.

mod environment;
mod login_shell;
#[cfg(test)]
mod tests;

pub(crate) use environment::MachineEnvironment;
pub(crate) use login_shell::LoginShell;
