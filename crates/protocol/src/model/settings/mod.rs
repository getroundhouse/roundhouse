//! What a session runs with, and changes to it: the model, the approval mode and the effort. Requests and launches
//! flatten them, so on the wire each setting is a field of its own.

#[cfg(test)]
use super::{Effort, ModelName, PermissionMode};

#[cfg(test)]
mod tests;

mod capabilities;
mod choices;
pub use choices::{SessionSettings, SessionSettingsChange};
