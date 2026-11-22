//! The environment the hub gives everything it starts on its machine, from the user's profile rather than from whoever
//! started the hub.

mod environment;
mod kept;
mod printing;
mod profile;
#[cfg(test)]
mod tests;

pub(crate) use environment::MachineEnvironment;
use kept::Vars;
pub use printing::{PRINTS_ENVIRONMENT, print_environment};
