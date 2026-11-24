//! Making a workspace from what the new-workspace dialog held, and sending its first chat the prompt. The hub does it
//! from what it keeps of the dialog, files and all. It runs on the thread `Client::start` gives it, since each call
//! blocks.

#[cfg(test)]
use crate::new_workspace::Request;
#[cfg(test)]
use client_core::{Client, FileToSend};
#[cfg(test)]
use protocol::client::new_workspace::{NewWorkspaceMade, UnsentPrompt};
#[cfg(test)]
use protocol::prompt::Prompt;

#[cfg(test)]
mod tests;

mod creation;
#[cfg(test)]
pub(super) use creation::Step;
pub(super) use creation::{GivenBack, Made, Unsent, make};
